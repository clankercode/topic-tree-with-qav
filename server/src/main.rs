use std::net::SocketAddr;

use tokio::net::TcpListener;
use tokio::signal;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3000);
    let database_path = std::env::var("DATABASE_PATH").unwrap_or_else(|_| "./dev.db".to_string());

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = TcpListener::bind(addr).await?;
    tracing::info!(%addr, %database_path, "server listening");

    let metrics = server::create_metrics();
    let db = if database_path == ":memory:" {
        server::Db::open_in_memory()?
    } else {
        server::Db::open_path(&database_path)?
    };
    let (state, writer_join) = server::AppState::new(db, metrics);

    let _excalidraw_reset_task = server::ws::spawn_excalidraw_scene_reset_task(state.clone());
    let _idle_reaper_task = spawn_idle_reaper(state.clone());

    axum::serve(listener, server::app_with_state(state.clone()))
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    // Graceful shutdown drain: dropping `state` releases this scope's
    // writer_tx clone. Any other clones (held by closed-out ws
    // sessions) drop as their futures finish. Then the writer's
    // `rx.recv()` returns None and the task exits.
    drop(state);
    match tokio::time::timeout(server::writer::SHUTDOWN_DRAIN_TIMEOUT, writer_join).await {
        Ok(Ok(())) => tracing::info!("writer task drained cleanly"),
        Ok(Err(e)) => tracing::error!(error = %e, "writer task join failed"),
        Err(_) => tracing::warn!(
            "writer task did not drain within {:?}",
            server::writer::SHUTDOWN_DRAIN_TIMEOUT
        ),
    }
    Ok(())
}

/// Spawn the idle-room reaper. Every 60 s, evict rooms that have had
/// no connected clients and no activity for at least 10 minutes. See
/// `.plan/2026-05-25-followup/risks.md` R21 + R28.
fn spawn_idle_reaper(state: server::AppState) -> tokio::task::JoinHandle<()> {
    use std::time::Duration;
    const TICK: Duration = Duration::from_secs(60);
    const IDLE_THRESHOLD_MS: i64 = 10 * 60 * 1000;
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(TICK);
        interval.tick().await; // skip the immediate first tick
        loop {
            interval.tick().await;
            let now = chrono_like_now_ms();
            let reaped = state.rooms.reap_idle(now, IDLE_THRESHOLD_MS);
            if !reaped.is_empty() {
                tracing::info!(
                    count = reaped.len(),
                    "reaped idle rooms",
                );
            }
        }
    })
}

fn chrono_like_now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut s) = signal::unix::signal(signal::unix::SignalKind::terminate()) {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("shutdown signal received");
}

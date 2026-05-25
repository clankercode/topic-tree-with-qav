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

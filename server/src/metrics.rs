use prometheus::{CounterVec, Encoder, IntCounter, IntGauge, Opts, Registry};
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct Metrics {
    registry: Registry,
    pub ws_connections_opened: IntCounter,
    pub ws_connections_closed: IntCounter,
    pub ws_messages_sent: IntCounter,
    pub http_requests: CounterVec,
    pub room_count: IntGauge,
}

impl Metrics {
    pub fn new() -> Self {
        let registry = Registry::new();

        let ws_connections_opened = IntCounter::with_opts(Opts::new(
            "ws_connections_opened_total",
            "Total number of WebSocket connections opened",
        ))
        .unwrap();

        let ws_connections_closed = IntCounter::with_opts(Opts::new(
            "ws_connections_closed_total",
            "Total number of WebSocket connections closed",
        ))
        .unwrap();

        let ws_messages_sent = IntCounter::with_opts(Opts::new(
            "ws_messages_sent_total",
            "Total number of WebSocket messages sent to clients",
        ))
        .unwrap();

        let http_requests = CounterVec::new(
            Opts::new("http_requests_total", "Total number of HTTP requests"),
            &["method", "path"],
        )
        .unwrap();

        let room_count =
            IntGauge::with_opts(Opts::new("room_count", "Number of active rooms")).unwrap();

        registry
            .register(Box::new(ws_connections_opened.clone()))
            .unwrap();
        registry
            .register(Box::new(ws_connections_closed.clone()))
            .unwrap();
        registry
            .register(Box::new(ws_messages_sent.clone()))
            .unwrap();
        registry.register(Box::new(http_requests.clone())).unwrap();
        registry.register(Box::new(room_count.clone())).unwrap();

        Self {
            registry,
            ws_connections_opened,
            ws_connections_closed,
            ws_messages_sent,
            http_requests,
            room_count,
        }
    }

    pub fn render(&self) -> String {
        let mut buffer = Vec::new();
        let gatherer = self.registry.gather();
        prometheus::TextEncoder::new()
            .encode(&gatherer, &mut buffer)
            .expect("metrics encoding should not fail");
        String::from_utf8(buffer).expect("metrics should be valid utf-8")
    }

    pub fn inc_room_count(&self) {
        self.room_count.inc();
    }

    pub fn dec_room_count(&self) {
        self.room_count.dec();
    }
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

pub type SharedMetrics = Arc<RwLock<Metrics>>;

pub fn create_metrics() -> SharedMetrics {
    Arc::new(RwLock::new(Metrics::new()))
}

use axum::{extract::Request, http::StatusCode, middleware::Next, response::Response};

use prometheus::{
    CounterVec, Encoder, Gauge, HistogramVec, TextEncoder, histogram_opts, opts,
    register_counter_vec, register_gauge, register_histogram_vec,
};

use std::sync::LazyLock;
use std::time::Instant;

static HTTP_REQUESTS_TOTAL: LazyLock<CounterVec> = LazyLock::new(|| {
    register_counter_vec!(
        opts!("http_requests_total", "Total number of HTTP requests"),
        &["method", "path", "status"]
    )
    .unwrap()
});

static HTTP_REQUEST_DURATION: LazyLock<HistogramVec> = LazyLock::new(|| {
    register_histogram_vec!(
        histogram_opts!(
            "http_request_duration_seconds",
            "HTTP request duration in seconds"
        )
        .buckets(vec![
            0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0
        ]),
        &["method", "path"]
    )
    .unwrap()
});

static WEBSOCKET_CONNECTIONS_ACTIVE: LazyLock<Gauge> = LazyLock::new(|| {
    register_gauge!(opts!(
        "websocket_connections_active",
        "Active WS connections"
    ))
    .unwrap()
});

static WEBSOCKET_MESSAGES_TOTAL: LazyLock<CounterVec> = LazyLock::new(|| {
    register_counter_vec!(
        opts!("websocket_messages_total", "Total WebSocket messages"),
        &["direction"]
    )
    .unwrap()
});

static CHAT_MESSAGES_TOTAL: LazyLock<CounterVec> = LazyLock::new(|| {
    register_counter_vec!(
        opts!("chat_messages_total", "Total chat messages processed"),
        &["message_type"]
    )
    .unwrap()
});

static CHAT_MESSAGE_PROCESSING_DURATION: LazyLock<HistogramVec> = LazyLock::new(|| {
    register_histogram_vec!(
        histogram_opts!(
            "chat_message_processing_duration_seconds",
            "Chat message processing duration"
        )
        .buckets(vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0]),
        &["message_type"]
    )
    .unwrap()
});

static GRPC_REQUESTS_TOTAL: LazyLock<CounterVec> = LazyLock::new(|| {
    register_counter_vec!(
        opts!("grpc_requests_total", "Total gRPC requests"),
        &["service", "method", "status"]
    )
    .unwrap()
});

static GRPC_REQUEST_DURATION: LazyLock<HistogramVec> = LazyLock::new(|| {
    register_histogram_vec!(
        histogram_opts!("grpc_request_duration_seconds", "gRPC request duration")
            .buckets(vec![0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]),
        &["service", "method"]
    )
    .unwrap()
});

static DB_QUERY_DURATION_SECONDS: LazyLock<HistogramVec> = LazyLock::new(|| {
    register_histogram_vec!(
        histogram_opts!(
            "db_query_duration_seconds",
            "Duration of database queries in seconds"
        ),
        &["operation"]
    )
    .unwrap()
});

pub async fn metrics_middleware(req: Request, next: Next) -> Response {
    let start = Instant::now();
    let method = req.method().to_string();
    let path = req.uri().path().to_string();

    let response = next.run(req).await;
    let duration = start.elapsed();
    let status = response.status().as_u16().to_string();

    HTTP_REQUESTS_TOTAL
        .with_label_values(&[&method, &path, &status])
        .inc();

    HTTP_REQUEST_DURATION
        .with_label_values(&[&method, &path])
        .observe(duration.as_secs_f64());

    response
}

// ------------------------------------------------------------
// METRICS ENDPOINT HANDLER
// ------------------------------------------------------------
pub async fn metrics_handler() -> Result<String, StatusCode> {
    let encoder = TextEncoder::new();
    let metrics = prometheus::gather();
    let mut buffer = Vec::new();

    encoder
        .encode(&metrics, &mut buffer)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    String::from_utf8(buffer).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

// ------------------------------------------------------------
// METRIC HELPERS
// ------------------------------------------------------------
pub struct Metrics;

impl Metrics {
    // --- WebSocket ---
    pub fn websocket_connected() {
        WEBSOCKET_CONNECTIONS_ACTIVE.inc();
    }

    pub fn websocket_disconnected() {
        WEBSOCKET_CONNECTIONS_ACTIVE.dec();
    }

    pub fn websocket_message_sent() {
        WEBSOCKET_MESSAGES_TOTAL.with_label_values(&["sent"]).inc();
    }

    pub fn websocket_message_received() {
        WEBSOCKET_MESSAGES_TOTAL
            .with_label_values(&["received"])
            .inc();
    }

    pub fn websocket_message_persisted() {
        WEBSOCKET_MESSAGES_TOTAL
            .with_label_values(&["peristed"])
            .inc();
    }

    // --- Chat ---
    pub fn chat_message_processed(msg_type: &str, duration: std::time::Duration) {
        CHAT_MESSAGES_TOTAL.with_label_values(&[msg_type]).inc();

        CHAT_MESSAGE_PROCESSING_DURATION
            .with_label_values(&[msg_type])
            .observe(duration.as_secs_f64());
    }

    pub fn grpc_request_completed(
        service: &str,
        method: &str,
        status: &str,
        duration: std::time::Duration,
    ) {
        GRPC_REQUESTS_TOTAL
            .with_label_values(&[service, method, status])
            .inc();

        GRPC_REQUEST_DURATION
            .with_label_values(&[service, method])
            .observe(duration.as_secs_f64());
    }

    pub fn observe_db_query(operation: &str, duration: std::time::Duration) {
        DB_QUERY_DURATION_SECONDS
            .with_label_values(&[operation])
            .observe(duration.as_secs_f64());
    }
}

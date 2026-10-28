use axum::{
    Router,
    extract::{State, WebSocketUpgrade},
    http::{HeaderName, HeaderValue},
    middleware,
    response::IntoResponse,
    routing::{any, get},
};
use std::sync::Arc;
use tower_http::set_header::SetResponseHeaderLayer;

use crate::{
    auth::AuthenticatedUser,
    handlers::conversation::getsert_conversation_id,
    metrics::{metrics_handler, metrics_middleware},
    socket::dm_socket,
    state::PerOxoState,
};

/// Frames larger than this are abuse: a chat message is at most
/// `validate::MAX_CONTENT_CHARS`, and the envelope is tiny.
const MAX_WS_MESSAGE_BYTES: usize = 64 * 1024;
const MAX_WS_FRAME_BYTES: usize = 16 * 1024;

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<PerOxoState>>,
    auth: AuthenticatedUser,
) -> impl IntoResponse {
    ws.max_message_size(MAX_WS_MESSAGE_BYTES)
        .max_frame_size(MAX_WS_FRAME_BYTES)
        .on_upgrade(move |socket| dm_socket(socket, auth.tenant_user_id, state))
}

fn security_headers() -> [(&'static str, &'static str); 6] {
    [
        ("x-content-type-options", "nosniff"),
        ("x-frame-options", "DENY"),
        ("referrer-policy", "no-referrer"),
        (
            "content-security-policy",
            "default-src 'none'; frame-ancestors 'none'",
        ),
        (
            "strict-transport-security",
            "max-age=31536000; includeSubDomains",
        ),
        ("cache-control", "no-store"),
    ]
}

fn apply_security_headers(app: Router) -> Router {
    let mut app = app;
    for (name, value) in security_headers() {
        app = app.layer(SetResponseHeaderLayer::overriding(
            HeaderName::from_static(name),
            HeaderValue::from_static(value),
        ));
    }
    app
}

pub fn peroxo_route(state: Arc<PerOxoState>) -> Router {
    apply_security_headers(
        Router::new()
            .route("/ws", any(ws_handler))
            .route("/conversations", get(getsert_conversation_id))
            .layer(middleware::from_fn(metrics_middleware))
            .with_state(state),
    )
}

/// Metrics live on their own listener so they are not reachable through the
/// public chat port. Scrape them from the internal network only.
pub fn metrics_route() -> Router {
    apply_security_headers(Router::new().route("/metrics", get(metrics_handler)))
}

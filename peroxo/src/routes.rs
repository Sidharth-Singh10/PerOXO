use axum::{
    Router,
    extract::{State, WebSocketUpgrade},
    middleware,
    response::IntoResponse,
    routing::{any, get},
};
use std::sync::Arc;

use crate::{
    auth::AuthenticatedUser,
    handlers::conversation::getsert_conversation_id,
    metrics::{metrics_handler, metrics_middleware},
    socket::dm_socket,
    state::PerOxoState,
};

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<PerOxoState>>,
    auth: AuthenticatedUser,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| dm_socket(socket, auth.tenant_user_id, state))
}

pub fn peroxo_route(state: Arc<PerOxoState>) -> Router {
    Router::new()
        .route("/ws", any(ws_handler))
        .route("/metrics", get(metrics_handler))
        .route("/conversations", get(getsert_conversation_id))
        .layer(middleware::from_fn(metrics_middleware))
        .with_state(state)
}

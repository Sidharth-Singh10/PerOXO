use axum::{
    Router,
    extract::{Query, State, WebSocketUpgrade},
    middleware,
    response::IntoResponse,
    routing::{any, get},
};
use std::{collections::HashMap, sync::Arc};

use crate::{
    metrics::{metrics_handler, metrics_middleware},
    socket::dm_socket,
    state::PerOxoState,
};

pub mod actors;
pub mod chat;
pub mod connections;
pub mod metrics;
#[cfg(feature = "mongo_db")]
pub mod mongo_db;
pub mod socket;
pub mod state;

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<PerOxoState>>,
    Query(params): Query<HashMap<String, i32>>,
) -> impl IntoResponse {
    let token = match params.get("token") {
        Some(token) => *token,
        None => return "Missing token".into_response(),
    };

    ws.on_upgrade(move |socket| dm_socket(socket, token, state))
}

pub fn peroxo_route(state: Arc<PerOxoState>) -> Router {
    Router::new()
        .route("/ws", any(ws_handler))
        .route("/metrics", get(metrics_handler))
        .layer(middleware::from_fn(metrics_middleware))
        .with_state(state)
}

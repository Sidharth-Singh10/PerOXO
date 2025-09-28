use axum::{
    Router,
    extract::{Query, State, WebSocketUpgrade},
    response::IntoResponse,
    routing::any,
};
use std::{collections::HashMap, sync::Arc};

use crate::{socket::dm_socket, state::PerOxoState};

#[cfg(feature = "persistence")]
use crate::connections::connect_chat_service_client;

pub mod actors;
pub mod chat;
pub mod connections;
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
        .with_state(state)
}

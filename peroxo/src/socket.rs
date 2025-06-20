use crate::state::AppState;
use axum::extract::ws::WebSocket;
use std::sync::Arc;

pub async fn dm_socket(socket: WebSocket, username: String, state: Arc<AppState>) {
    state
        .connection_manager
        .handle_connection(socket, username)
        .await;
}

use crate::state::AppState;
use axum::extract::ws::WebSocket;
use std::sync::Arc;

pub async fn dm_socket(socket: WebSocket, user_id: i32, state: Arc<AppState>) {
    state
        .connection_manager
        .handle_connection(socket, user_id)
        .await;
}

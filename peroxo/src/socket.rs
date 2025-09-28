use crate::state::PerOxoState;
use axum::extract::ws::WebSocket;
use std::sync::Arc;

pub async fn dm_socket(socket: WebSocket, user_id: i32, state: Arc<PerOxoState>) {
    state
        .connection_manager
        .handle_connection(socket, user_id)
        .await;
}

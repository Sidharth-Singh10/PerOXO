use crate::{UserToken, state::PerOxoState};
use axum::extract::ws::WebSocket;
use std::sync::Arc;

pub async fn dm_socket(socket: WebSocket, user_token: UserToken, state: Arc<PerOxoState>) {
    state
        .connection_manager
        .handle_connection(socket, user_token)
        .await;
}

use crate::{state::PerOxoState, tenant::TenantUserId};
use axum::extract::ws::WebSocket;
use std::sync::Arc;

pub async fn dm_socket(socket: WebSocket, tenant_user_id: TenantUserId, state: Arc<PerOxoState>) {
    state
        .connection_manager
        .handle_connection(socket, tenant_user_id)
        .await;
}

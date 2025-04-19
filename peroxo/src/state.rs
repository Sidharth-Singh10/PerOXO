use std::{collections::HashMap, sync::Arc};

use crate::chat::ChatMessage;
use crate::state::matcher::user_service_client::UserServiceClient;
use axum::{Json, extract::State, response::IntoResponse};
use tokio::sync::{Mutex, broadcast};

pub mod matcher {
    tonic::include_proto!("user_service");
}

pub struct AppState {
    // Map of username to their broadcast channel
    pub users: Mutex<HashMap<String, broadcast::Sender<ChatMessage>>>,
    // Track who's online
    pub online_users: Mutex<Vec<String>>,

    pub matcher_client: UserServiceClient<tonic::transport::Channel>,
}

// Broadcast presence updates to all users
pub async fn broadcast_presence(state: &Arc<AppState>, presence_msg: &ChatMessage) {
    let users = state.users.lock().await;
    for tx in users.values() {
        let _ = tx.send(presence_msg.clone());
    }
}

// Get list of online users
pub async fn get_online_users(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let online_users = state.online_users.lock().await;
    Json(online_users.clone())
}

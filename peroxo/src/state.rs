use std::{collections::HashMap, sync::Arc};

use crate::chat::ChatMessage;
use crate::state::user_service::user_service_client::UserServiceClient;
use axum::{Json, extract::State, response::IntoResponse};
use tokio::sync::{Mutex, broadcast};
use user_service::GetMatchedUsersRequest;

pub mod user_service {
    tonic::include_proto!("user_service");
}
pub mod chat_service {
    tonic::include_proto!("chat_service");
}

pub struct AppState {
    // Map of username to their broadcast channel
    pub users: Mutex<HashMap<String, broadcast::Sender<ChatMessage>>>,
    // Track who's online
    pub online_users: Mutex<Vec<String>>,

    pub user_service_client: UserServiceClient<tonic::transport::Channel>,
}

// Fix the conversion of username to i32 (We operate on user IDs not on usernames)
// Broadcast presence updates only to user's friends
pub async fn broadcast_presence(state: &Arc<AppState>, presence_msg: &ChatMessage, user_id: i32) {
    // Get the user's friends via gRPC call
    let mut client = state.user_service_client.clone();

    let request = tonic::Request::new(GetMatchedUsersRequest { user_id });

    match client.get_matched_users(request).await {
        Ok(response) => {
            let matched_user_ids = response.into_inner().matched_user_ids;
            let users = state.users.lock().await;

            // Convert user IDs to usernames (assuming you have a mapping or the IDs are the usernames as strings)
            for matched_id in matched_user_ids {
                let username = matched_id.to_string();

                if let Some(tx) = users.get(&username) {
                    let _ = tx.send(presence_msg.clone());
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to get matched users: {:?}", e);
            // Handle error - maybe log it or take alternative action
        }
    }
}

// Get list of online users
pub async fn get_online_users(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let online_users = state.online_users.lock().await;
    Json(online_users.clone())
}

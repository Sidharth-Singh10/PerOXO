use crate::actors::{
    chat_service::chat_service_client::ChatServiceClient,
    connection_manager::ConnectionManager,
    message_router::{MessageRouter, RouterMessage},
    persistance_actor::PersistenceActor,
};
use axum::{
    Json,
    extract::{Query, State},
    response::IntoResponse,
};
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};
use tonic::transport::Channel;
use tracing::error;

use crate::user_service::user_service_client::UserServiceClient;
use crate::user_service::{GetMatchedUsersRequest, GetMatchedUsersResponse};

pub struct AppState {
    pub connection_manager: Arc<ConnectionManager>,
    pub router_sender: mpsc::UnboundedSender<RouterMessage>,
    pub user_service_client: UserServiceClient<Channel>,
}

impl AppState {
    pub async fn new(
        chat_service_client: ChatServiceClient<Channel>,
        user_service_client: UserServiceClient<Channel>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let (persistence_actor, persistence_sender) =
            PersistenceActor::new(chat_service_client).await?;
        let (router, router_sender) = MessageRouter::new(persistence_sender);
        let connection_manager = Arc::new(ConnectionManager::new(router_sender.clone()));

        // Spawn actors
        tokio::spawn(router.run());
        tokio::spawn(persistence_actor.run());

        Ok(Self {
            connection_manager,
            router_sender,
            user_service_client,
        })
    }
}

#[derive(Deserialize)]
pub struct OnlineMatchesQuery {
    user_id: i32,
}

// Response structure
#[derive(serde::Serialize)]
pub struct OnlineMatchesResponse {
    pub matched_users: Vec<i32>,
    pub online_matched_users: Vec<i32>,
}

// Get online status of matched users for a specific user
pub async fn get_online_matched_users(
    State(state): State<Arc<AppState>>,
    Query(params): Query<OnlineMatchesQuery>,
) -> impl IntoResponse {
    // First, get all online users from the router
    let (respond_to, response) = oneshot::channel();
    let msg = RouterMessage::GetOnlineUsers { respond_to };

    if state.router_sender.send(msg).is_err() {
        error!("Failed to communicate with message router");
        return Json(OnlineMatchesResponse {
            matched_users: Vec::new(),
            online_matched_users: Vec::new(),
        });
    }

    let online_users = match response.await {
        Ok(users) => users,
        Err(_) => {
            error!("Failed to get response from message router");
            return Json(OnlineMatchesResponse {
                matched_users: Vec::new(),
                online_matched_users: Vec::new(),
            });
        }
    };

    // Get matched users from the user service
    let mut user_service_client = state.user_service_client.clone();
    let request = tonic::Request::new(GetMatchedUsersRequest {
        user_id: params.user_id,
    });

    let matched_users = match user_service_client.get_matched_users(request).await {
        Ok(response) => response.into_inner().matched_user_ids,
        Err(e) => {
            error!("Failed to get matched users from user service: {}", e);
            return Json(OnlineMatchesResponse {
                matched_users: Vec::new(),
                online_matched_users: Vec::new(),
            });
        }
    };

    // Filter online users to only include matched users
    let online_matched_users: Vec<i32> = matched_users
        .iter()
        .filter(|&user_id| online_users.contains(user_id))
        .cloned()
        .collect();

    Json(OnlineMatchesResponse {
        matched_users,
        online_matched_users,
    })
}

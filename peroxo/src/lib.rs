use axum::{
    Router,
    extract::{Query, State, WebSocketUpgrade},
    http::{self, Method},
    response::IntoResponse,
    routing::{any, get},
};
use std::{collections::HashMap, sync::Arc};
use tower_http::cors::{AllowOrigin, CorsLayer};

use crate::{
    actors::chat_service::chat_service_client::ChatServiceClient,
    socket::dm_socket,
    state::{AppState, get_online_matched_users},
};

pub mod actors;
pub mod chat;
pub mod socket;
pub mod state;
pub mod user_service {
    tonic::include_proto!("user_service");
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
    Query(params): Query<HashMap<String, i32>>,
) -> impl IntoResponse {
    let token = match params.get("token") {
        Some(token) => *token,
        None => return "Missing token".into_response(),
    };

    ws.on_upgrade(move |socket| dm_socket(socket, token, state))
}

pub async fn build_app() -> Router {
    let chat_service_addr = std::env::var("CHAT_SERVICE_ADDR").unwrap();
    let chat_service_client = ChatServiceClient::connect(chat_service_addr).await.unwrap();

    let user_service_addr = std::env::var("USER_SERVICE_ADDR").unwrap();
    let user_service_client =
        user_service::user_service_client::UserServiceClient::connect(user_service_addr)
            .await
            .unwrap();

    let state = AppState::new(chat_service_client, user_service_client)
        .await
        .unwrap();

    let cors = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
        .allow_headers([
            http::header::ACCEPT,
            http::header::CONTENT_TYPE,
            http::header::AUTHORIZATION,
            http::header::ORIGIN,
            http::header::SET_COOKIE,
        ])
        .allow_origin(AllowOrigin::any());

    Router::new()
        .route("/ws", any(ws_handler))
        .route("/users/online", get(get_online_matched_users))
        .layer(cors)
        .with_state(state.into())
}

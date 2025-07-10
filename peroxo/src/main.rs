use axum::{
    Router,
    extract::{Query, State, WebSocketUpgrade},
    http::{self, Method},
    response::IntoResponse,
    routing::{any, get},
};
use state::AppState;
use std::{collections::HashMap, net::SocketAddr, sync::Arc};
use tower_http::cors::{AllowOrigin, CorsLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::{
    actors::chat_service::chat_service_client::ChatServiceClient, socket::dm_socket,
    state::get_online_matched_users,
};

mod actors;
mod chat;
mod socket;
mod state;
pub mod user_service {
    tonic::include_proto!("user_service");
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
    Query(params): Query<HashMap<String, i32>>,
) -> impl IntoResponse {
    // Need proper authentication here
    let token = match params.get("token") {
        Some(token) => *token,
        None => {
            return "Missing token".into_response();
        }
    };

    ws.on_upgrade(move |socket| dm_socket(socket, token, state))
}

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();

    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "debug".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let chat_service_addr = std::env::var("CHAT_SERVICE_ADDR").unwrap();
    tracing::info!("Connecting to chat service at {}", chat_service_addr);

    let chat_service_client = ChatServiceClient::connect(chat_service_addr).await.unwrap();

    let user_service_addr = std::env::var("USER_SERVICE_ADDR").unwrap();
    tracing::info!("Connecting to user service at {}", user_service_addr);
    let user_service_client =
        user_service::user_service_client::UserServiceClient::connect(user_service_addr)
            .await
            .unwrap();

    // Initialize application state with actor system
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

    let app = Router::new()
        .route("/ws", any(ws_handler))
        .route("/users/online", get(get_online_matched_users))
        .layer(cors)
        .with_state(state.into());

    let per_oxo_service_addr = std::env::var("PER_OXO_SERVICE_ADDR").unwrap();
    let listener = tokio::net::TcpListener::bind(&per_oxo_service_addr)
        .await
        .unwrap();

    tracing::info!(
        "Per-OXO service listening on {} {:?}",
        per_oxo_service_addr,
        listener
    );

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .unwrap();
}

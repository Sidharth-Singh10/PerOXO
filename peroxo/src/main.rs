use crate::state::user_service::user_service_client::UserServiceClient;
use axum::{
    Router,
    extract::{State, ws::WebSocketUpgrade},
    http::{self, Method},
    response::IntoResponse,
    routing::{any, get},
};
use socket::dm_socket;
use state::{AppState, get_online_users};
use std::{collections::HashMap, net::SocketAddr, sync::Arc};
use tokio::sync::Mutex;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod chat;
mod socket;
mod state;

// WebSocket handler with authentication
async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
    axum::extract::Query(params): axum::extract::Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let token = params.get("token").cloned().unwrap_or_default();

    // Reject connection if token is empty
    if token.is_empty() {
        return "Missing token".into_response();
    }

    ws.on_upgrade(move |socket| dm_socket(socket, token, state))
}

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let user_service_client = UserServiceClient::connect("http://[::1]:50051")
        .await
        .expect("Failed to connect to gRPC matcher service");

    let state = Arc::new(AppState {
        users: Mutex::new(HashMap::new()),
        online_users: Mutex::new(Vec::new()),
        user_service_client,
    });

    let cors = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
        .allow_headers([
            http::header::ACCEPT,
            http::header::CONTENT_TYPE,
            http::header::AUTHORIZATION,
            http::header::ORIGIN,
            http::header::SET_COOKIE,
        ])
        // .allow_credentials(true)
        .allow_origin(AllowOrigin::any());

    let app = Router::new()
        .route("/ws", any(ws_handler))
        .route("/users/online", get(get_online_users))
        .layer(cors)
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .unwrap();
}

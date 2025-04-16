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
    // Extract the token from the query parameter
    axum::extract::Query(params): axum::extract::Query<HashMap<String, String>>,
) -> impl IntoResponse {
    // Get the token (username)
    let token = params.get("token").cloned().unwrap_or_default();

    // Reject connection if token is empty
    if token.is_empty() {
        return "Missing token".into_response();
    }

    // Upgrade the connection
    ws.on_upgrade(move |socket| dm_socket(socket, token, state))
}

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Set up shared state
    let state = Arc::new(AppState {
        users: Mutex::new(HashMap::new()),
        online_users: Mutex::new(Vec::new()),
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

    // Build our application with a route
    let app = Router::new()
        // WebSocket endpoint
        .route("/ws", any(ws_handler))
        // Get online users endpoint
        .route("/users/online", get(get_online_users))
        .layer(cors)
        .with_state(state);

    // Run it
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

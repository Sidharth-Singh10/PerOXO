use axum::{
    Router,
    extract::{Query, State, WebSocketUpgrade},
    http::{self, Method},
    response::IntoResponse,
    routing::{any, get},
};
use state::{AppState, get_online_users};
use std::{collections::HashMap, net::SocketAddr, sync::Arc};
use tower_http::cors::{AllowOrigin, CorsLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::socket::dm_socket;

mod actors;
mod chat;
mod socket;
mod state;

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    // Need proper authentication here
    let token = params.get("token").cloned().unwrap_or_default();

    if token.is_empty() {
        return "Missing token".into_response();
    }

    ws.on_upgrade(move |socket| dm_socket(socket, token, state))
}

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();

    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Initialize application state with actor system
    let state = Arc::new(AppState::new());

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
        .route("/users/online", get(get_online_users))
        .layer(cors)
        .with_state(state);

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

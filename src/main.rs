use axum::{
    Json, Router,
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{self, Method},
    response::IntoResponse,
    routing::{any, get},
};
use chat::{ChatMessage, PresenceStatus};
use futures::SinkExt;
use futures::StreamExt;
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, Mutex},
};
use tokio::sync::broadcast;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod chat;

struct AppState {
    // Map of username to their broadcast channel
    users: Mutex<HashMap<String, broadcast::Sender<ChatMessage>>>,
    // Track who's online
    online_users: Mutex<Vec<String>>,
}

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
    ws.on_upgrade(move |socket| handle_socket(socket, token, state))
}

async fn handle_socket(mut socket: WebSocket, username: String, state: Arc<AppState>) {
    // Add user to online users
    {
        let mut online_users = state.online_users.lock().unwrap();
        online_users.push(username.clone());

        // Broadcast that this user is now online
        let presence_msg = ChatMessage::Presence {
            user: username.clone(),
            status: PresenceStatus::Online,
        };
        broadcast_presence(&state, &presence_msg);
    }

    // Get the user's channel
    let rx = {
        let users = state.users.lock().unwrap();
        users.get(&username).map(|tx| tx.subscribe())
    };

    let mut rx = match rx {
        Some(rx) => rx,
        None => {
            let _ = socket.close().await;
            return;
        }
    };

    // Split socket for concurrent read/write
    let (mut sender, mut receiver) = socket.split();

    // Spawn task to forward messages from broadcast to the WebSocket
    let mut send_task = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            let json = serde_json::to_string(&msg).unwrap();
            if sender.send(Message::Text(json.into())).await.is_err() {
                break;
            }
        }
    });

    // Handle incoming messages from WebSocket
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(Message::Text(text))) = receiver.next().await {
            match serde_json::from_str::<ChatMessage>(&text) {
                Ok(ChatMessage::DirectMessage { from, to, content }) => {
                    // Ensure the sender is who they claim to be
                    if from != username {
                        continue; // Ignore spoofed messages
                    }

                    // Forward the message to the recipient
                    let users = state.users.lock().unwrap();
                    if let Some(tx) = users.get(&to) {
                        let _ = tx.send(ChatMessage::DirectMessage { from, to, content });
                    }
                }
                _ => {} // Ignore other message types from clients
            }
        }

        // User disconnected - update presence
        let mut online_users = state.online_users.lock().unwrap();
        if let Some(pos) = online_users.iter().position(|u| u == &username) {
            online_users.remove(pos);
        }

        // Broadcast that this user is now offline
        let presence_msg = ChatMessage::Presence {
            user: username,
            status: PresenceStatus::Offline,
        };
        broadcast_presence(&state, &presence_msg);
    });

    // Wait for either task to finish
    tokio::select! {
        _ = &mut send_task => recv_task.abort(),
        _ = &mut recv_task => send_task.abort(),
    }
}

// Broadcast presence updates to all users
fn broadcast_presence(state: &Arc<AppState>, presence_msg: &ChatMessage) {
    let users = state.users.lock().unwrap();
    for tx in users.values() {
        let _ = tx.send(presence_msg.clone());
    }
}

// Get list of online users
async fn get_online_users(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let online_users = state.online_users.lock().unwrap();
    Json(online_users.clone())
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

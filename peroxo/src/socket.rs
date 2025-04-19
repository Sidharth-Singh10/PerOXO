use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket};
use futures::{SinkExt, StreamExt};

use crate::{
    chat::{ChatMessage, PresenceStatus},
    state::{AppState, broadcast_presence},
};

pub async fn dm_socket(mut socket: WebSocket, username: String, state: Arc<AppState>) {
    // Add user to online users
    {
        let mut online_users = state.online_users.lock().await;
        online_users.push(username.clone());

        // Broadcast that this user is now online
        let presence_msg = ChatMessage::Presence {
            user: username.clone(),
            status: PresenceStatus::Online,
        };

        // Fix the conversion of username to i32 (We operate on user IDs not on usernames)
        broadcast_presence(&state, &presence_msg, username.parse::<i32>().unwrap()).await;
    }

    // Get the user's channel
    let rx = {
        let users = state.users.lock().await;
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
                    let users = state.users.lock().await;
                    if let Some(tx) = users.get(&to) {
                        let _ = tx.send(ChatMessage::DirectMessage { from, to, content });
                    }
                }
                _ => {} // Ignore other message types from clients
            }
        }

        // User disconnected - update presence
        let mut online_users = state.online_users.lock().await;
        if let Some(pos) = online_users.iter().position(|u| u == &username) {
            online_users.remove(pos);
        }

        // Broadcast that this user is now offline
        let presence_msg = ChatMessage::Presence {
            user: username.clone(),
            status: PresenceStatus::Offline,
        };

        // Fix the conversion of username to i32 (We operate on user IDs not on usernames)
        broadcast_presence(&state, &presence_msg, username.parse::<i32>().unwrap()).await;
    });

    // Wait for either task to finish
    tokio::select! {
        _ = &mut send_task => recv_task.abort(),
        _ = &mut recv_task => send_task.abort(),
    }
}

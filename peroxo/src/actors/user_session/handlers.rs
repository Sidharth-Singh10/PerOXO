use crate::actors::{message_router::RouterMessage, uuid_util::NODE_ID};
use crate::chat::{ChatMessage, MessageStatus};
use crate::metrics::Metrics;
use crate::tenant::TenantUserId;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, error};
use uuid::Uuid;

const PERSIST_ACK_TIMEOUT: Duration = Duration::from_secs(20);

pub async fn handle_direct_message(
    conversation_id: String,
    user_token: TenantUserId,
    to: TenantUserId,
    content: String,
    client_message_id: Uuid,
    router_sender: &mpsc::Sender<RouterMessage>,
    ack_sender: &mpsc::Sender<ChatMessage>,
) -> Result<(), Box<dyn std::error::Error>> {
    Metrics::websocket_message_received();

    let server_message_id = Uuid::now_v1(&NODE_ID);

    // Tenant isolation: a sender may only message users inside its own
    // project. Cross-tenant payloads are rejected with an explicit Failed
    // ack instead of being forwarded to the router.
    if to.project_id != user_token.project_id {
        error!(
            "User {} attempted cross-tenant message to {}",
            user_token, to
        );
        let ack_message = ChatMessage::MessageAck {
            client_message_id,
            message_id: server_message_id,
            timestamp: chrono::Utc::now().timestamp_millis(),
            status: MessageStatus::Failed("Cross-tenant recipient rejected".to_string()),
        };
        let _ = ack_sender.send(ack_message).await;
        return Ok(());
    }

    let (respond_to, response) = oneshot::channel();

    let router_msg = RouterMessage::SendDirectMessage {
        conversation_id,
        from: user_token.clone(),
        to,
        content,
        message_id: server_message_id,
        respond_to: Some(respond_to),
    };

    if router_sender.send(router_msg).await.is_err() {
        error!("Failed to send message to router for user {}", user_token);
        return Err("Router communication failed".into());
    }

    let ack_sender_clone = ack_sender.clone();
    tokio::spawn(async move {
        match tokio::time::timeout(PERSIST_ACK_TIMEOUT, response).await {
            Ok(Ok(ack_response)) => {
                let ack_message = ChatMessage::MessageAck {
                    client_message_id,
                    message_id: ack_response.message_id,
                    timestamp: ack_response.timestamp,
                    status: ack_response.status,
                };

                if let Err(e) = ack_sender_clone.send(ack_message).await {
                    error!("Failed to send acknowledgment message: {}", e);
                }
            }
            Ok(Err(_)) => {
                debug!("Persistence response channel closed before ack");
            }
            Err(_) => {
                error!("Persistence ack timed out for message {}", server_message_id);
                let ack_message = ChatMessage::MessageAck {
                    client_message_id,
                    message_id: server_message_id,
                    timestamp: chrono::Utc::now().timestamp_millis(),
                    status: crate::chat::MessageStatus::Failed("Persistence timeout".to_string()),
                };
                let _ = ack_sender_clone.send(ack_message).await;
            }
        }
    });

    debug!(
        "Direct message handled successfully for user {}",
        user_token
    );
    Ok(())
}
// Add to handlers module in user_session:
pub async fn handle_room_message(
    user_id: TenantUserId,
    room_id: String,
    from: TenantUserId,
    content: String,
    client_message_id: uuid::Uuid,
    router_sender: &mpsc::Sender<RouterMessage>,
    ack_sender: &mpsc::Sender<ChatMessage>,
) -> Result<(), String> {
    if from != user_id {
        return Err("User ID mismatch".to_string());
    }

    let server_message_id = Uuid::now_v1(&NODE_ID);

    let (respond_to, response) = oneshot::channel();
    let router_msg = RouterMessage::SendRoomMessage {
        room_id,
        from,
        content,
        message_id: server_message_id,
        respond_to: Some(respond_to),
    };

    router_sender
        .send(router_msg)
        .await
        .map_err(|_| "Failed to send to router".to_string())?;

    let ack_sender = ack_sender.clone();
    tokio::spawn(async move {
        match tokio::time::timeout(PERSIST_ACK_TIMEOUT, response).await {
            Ok(Ok(ack_response)) => {
                let ack_msg = ChatMessage::MessageAck {
                    client_message_id,
                    message_id: ack_response.message_id,
                    timestamp: ack_response.timestamp,
                    status: ack_response.status,
                };
                let _ = ack_sender.send(ack_msg).await;
            }
            Ok(Err(_)) => {
                debug!("Room persistence response channel closed before ack");
            }
            Err(_) => {
                error!("Room persistence ack timed out for message {}", server_message_id);
                let ack_msg = ChatMessage::MessageAck {
                    client_message_id,
                    message_id: server_message_id,
                    timestamp: chrono::Utc::now().timestamp_millis(),
                    status: crate::chat::MessageStatus::Failed("Persistence timeout".to_string()),
                };
                let _ = ack_sender.send(ack_msg).await;
            }
        }
    });

    Ok(())
}

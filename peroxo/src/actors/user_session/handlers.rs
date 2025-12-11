use crate::actors::{message_router::RouterMessage, uuid_util::NODE_ID};
use crate::chat::ChatMessage;
use crate::metrics::Metrics;
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, error, warn};
use uuid::Uuid;

pub async fn handle_direct_message(
    user_id: i32,
    from: i32,
    to: i32,
    content: String,
    router_sender: &mpsc::UnboundedSender<RouterMessage>,
    ack_sender: &mpsc::Sender<ChatMessage>,
) -> Result<(), Box<dyn std::error::Error>> {
    Metrics::websocket_message_received();
    if from != user_id {
        warn!("User {} attempted to spoof message from {}", user_id, from);
        return Ok(()); // Not an error, just invalid input
    }

    let (respond_to, response) = oneshot::channel();

    let message_id = Uuid::now_v1(&NODE_ID);

    let router_msg = RouterMessage::SendDirectMessage {
        from,
        to,
        content,
        message_id,
        respond_to: Some(respond_to),
    };

    if router_sender.send(router_msg).is_err() {
        error!("Failed to send message to router for user {}", user_id);
        return Err("Router communication failed".into());
    }

    let ack_sender_clone = ack_sender.clone();
    tokio::spawn(async move {
        if let Ok(ack_response) = response.await {
            let ack_message = ChatMessage::MessageAck {
                message_id: ack_response.message_id,
                timestamp: ack_response.timestamp,
                status: ack_response.status,
            };

            if let Err(e) = ack_sender_clone.send(ack_message).await {
                error!("Failed to send acknowledgment message: {}", e);
            }
        }
    });

    debug!("Direct message handled successfully for user {}", user_id);
    Ok(())
}
// Add to handlers module in user_session:
pub async fn handle_room_message(
    user_id: i32,
    room_id: String,
    from: i32,
    content: String,
    message_id: uuid::Uuid,
    router_sender: &mpsc::UnboundedSender<RouterMessage>,
    ack_sender: &mpsc::Sender<ChatMessage>,
) -> Result<(), String> {
    if from != user_id {
        return Err("User ID mismatch".to_string());
    }

    let (respond_to, response) = oneshot::channel();
    let router_msg = RouterMessage::SendRoomMessage {
        room_id,
        from,
        content,
        message_id,
        respond_to: Some(respond_to),
    };

    router_sender
        .send(router_msg)
        .map_err(|_| "Failed to send to router".to_string())?;

    let ack_sender = ack_sender.clone();
    tokio::spawn(async move {
        if let Ok(ack_response) = response.await {
            let ack_msg = ChatMessage::MessageAck {
                message_id: ack_response.message_id,
                timestamp: ack_response.timestamp,
                status: ack_response.status,
            };
            let _ = ack_sender.send(ack_msg).await;
        }
    });

    Ok(())
}

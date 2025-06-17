use crate::queries::DirectMessage;
use lapin::{
    BasicProperties, Channel, Connection,
    options::{BasicPublishOptions, ConfirmSelectOptions, QueueDeclareOptions},
    publisher_confirm::Confirmation,
    types::FieldTable,
};
use scylla::value::CqlTimestamp;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub struct MessagePublisher {
    channel: Channel,
    queue_name: String,
}

impl MessagePublisher {
    pub async fn new(connection: &Connection, queue_name: String) -> lapin::Result<Self> {
        let channel = connection.create_channel().await?;

        // Enable publisher confirmations
        channel
            .confirm_select(ConfirmSelectOptions::default())
            .await?;

        // Declare the queue
        channel
            .queue_declare(
                &queue_name,
                QueueDeclareOptions {
                    durable: true,
                    ..Default::default()
                },
                FieldTable::default(),
            )
            .await?;

        Ok(Self {
            channel,
            queue_name,
        })
    }

    pub async fn publish_message(
        &self,
        message: &SerializableDirectMessage,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let payload = serde_json::to_vec(message)?;

        let confirm = self
            .channel
            .basic_publish(
                "",
                &self.queue_name,
                BasicPublishOptions::default(),
                &payload,
                BasicProperties::default().with_delivery_mode(2), // Make message persistent
            )
            .await?
            .await?;

        match confirm {
            Confirmation::Ack(_) => Ok(()),
            Confirmation::Nack(_) => Err("Message was nacked by RabbitMQ".into()),
            Confirmation::NotRequested => Err("Publisher confirmation was not requested".into()),
        }
    }

    #[allow(dead_code)]
    pub async fn publish_message_no_confirm(
        &self,
        message: &SerializableDirectMessage,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let payload = serde_json::to_vec(message)?;

        self.channel
            .basic_publish(
                "",
                &self.queue_name,
                BasicPublishOptions::default(),
                &payload,
                BasicProperties::default().with_delivery_mode(2), // Make message persistent
            )
            .await?;

        Ok(())
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SerializableDirectMessage {
    pub conversation_id: String,
    pub message_id: Uuid,
    pub sender_id: i32,
    pub recipient_id: i32,
    pub message_text: String,
    pub created_at: i64,
}

impl From<DirectMessage> for SerializableDirectMessage {
    fn from(dm: DirectMessage) -> Self {
        Self {
            conversation_id: dm.conversation_id,
            message_id: dm.message_id,
            sender_id: dm.sender_id,
            recipient_id: dm.recipient_id,
            message_text: dm.message_text,
            created_at: dm.created_at.0,
        }
    }
}

impl From<SerializableDirectMessage> for DirectMessage {
    fn from(sdm: SerializableDirectMessage) -> Self {
        Self {
            conversation_id: sdm.conversation_id,
            message_id: sdm.message_id,
            sender_id: sdm.sender_id,
            recipient_id: sdm.recipient_id,
            message_text: sdm.message_text,
            created_at: CqlTimestamp(sdm.created_at),
        }
    }
}

use futures_lite::stream::StreamExt;
use lapin::{
    Channel, Connection,
    options::{BasicAckOptions, BasicConsumeOptions, BasicQosOptions, QueueDeclareOptions},
    types::FieldTable,
};
use scylla::{client::session::Session, value::CqlTimestamp};
use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::scylla_db::{DirectMessage, write_direct_message};

pub struct MessageConsumer {
    channel: Channel,
    session: Session,
    queue_name: String,
}

impl MessageConsumer {
    pub async fn new(
        rabbitmq_connection: &Connection,
        scylla_session: Session,
        queue_name: String,
    ) -> lapin::Result<Self> {
        let channel = rabbitmq_connection.create_channel().await?;

        // Set QoS to process one message at a time
        channel.basic_qos(1, BasicQosOptions::default()).await?;

        // Declare the queue (ensure it exists)
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
            session: scylla_session,
            queue_name,
        })
    }

    pub async fn start_consuming(&self) -> Result<(), Box<dyn std::error::Error>> {
        info!(
            "Starting to consume messages from queue: {}",
            self.queue_name
        );

        let mut consumer = self
            .channel
            .basic_consume(
                &self.queue_name,
                "dm_consumer",
                BasicConsumeOptions::default(),
                FieldTable::default(),
            )
            .await?;

        while let Some(delivery_result) = consumer.next().await {
            match delivery_result {
                Ok(delivery) => {
                    match self.process_message(&delivery.data).await {
                        Ok(_) => {
                            info!("Successfully processed message");
                            // Acknowledge the message
                            if let Err(e) = delivery.ack(BasicAckOptions::default()).await {
                                error!("Failed to acknowledge message: {}", e);
                            }
                        }
                        Err(e) => {
                            error!("Failed to process message: {}", e);
                            // You might want to implement retry logic or dead letter queue here
                            // For now, we'll just acknowledge to prevent infinite redelivery
                            if let Err(ack_err) = delivery.ack(BasicAckOptions::default()).await {
                                error!("Failed to acknowledge failed message: {}", ack_err);
                            }
                        }
                    }
                }
                Err(e) => {
                    error!("Error receiving message: {}", e);
                    // Small delay before continuing to prevent tight error loops
                    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                }
            }
        }

        warn!("Consumer ended");
        Ok(())
    }

    async fn process_message(&self, data: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
        // Deserialize the message
        let serializable_message: SerializableDirectMessage = serde_json::from_slice(data)?;

        info!(
            "Processing message: {} from {} to {}",
            serializable_message.message_id,
            serializable_message.sender_id,
            serializable_message.recipient_id
        );

        // Convert to DirectMessage
        let direct_message: DirectMessage = serializable_message.into();

        // Write to ScyllaDB using your existing function
        write_direct_message(&self.session, direct_message)
            .await
            .unwrap();

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

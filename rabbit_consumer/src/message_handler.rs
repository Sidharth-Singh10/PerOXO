use futures_lite::stream::StreamExt;
use lapin::{
    Channel, Connection,
    options::{BasicAckOptions, BasicConsumeOptions, BasicQosOptions, QueueDeclareOptions},
    types::FieldTable,
};

use tracing::{error, info, warn};

#[async_trait::async_trait]
pub trait MessageHandler: Send + Sync {
    async fn process_message(&self, data: &[u8]) -> Result<(), Box<dyn std::error::Error>>;
}

pub struct MessageConsumer<H: MessageHandler> {
    channel: Channel,
    queue_name: String,
    handler: H,
}

impl<H: MessageHandler> MessageConsumer<H> {
    pub async fn new(
        rabbitmq_connection: &Connection,
        queue_name: String,
        handler: H,
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
            queue_name,
            handler,
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
                format!("{}_consumer", self.queue_name).as_str(),
                BasicConsumeOptions::default(),
                FieldTable::default(),
            )
            .await?;

        while let Some(delivery_result) = consumer.next().await {
            match delivery_result {
                Ok(delivery) => {
                    match self.handler.process_message(&delivery.data).await {
                        Ok(_) => {
                            info!("Successfully processed message for queue: {}", self.queue_name);
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
}

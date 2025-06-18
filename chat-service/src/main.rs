use crate::chat_services::ChatServiceImpl;
use crate::rabbit::MessagePublisher;
use lapin::Connection;
use lapin::ConnectionProperties;
use scylla::client::session_builder::SessionBuilder;
use scylla::client::{execution_profile::ExecutionProfile, session::Session};
use scylla::statement::Consistency;
use std::env;
use std::error::Error;
use std::sync::Arc;
use tonic::transport::Server;
use tonic_health::server::health_reporter;
mod chat_services;
mod queries;
mod rabbit;
use crate::chat_service::chat_service_server::ChatServiceServer;
use crate::migrations::migrations::run_database_migrations;
pub mod chat_service {
    tonic::include_proto!("chat_service");
}
mod migrations;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let scylla_host = env::var("SCYLLA_HOST").unwrap_or_else(|_| "127.0.0.1:9042".to_string());
    let rabbitmq_url =
        env::var("RABBITMQ_URL").unwrap_or_else(|_| "amqp://localhost:5672".to_string());

    let grpc_addr = env::var("GRPC_ADDR")?;

    println!("Starting chat service...");
    println!("ScyllaDB host: {}", scylla_host);
    println!("RabbitMQ URL: {}", rabbitmq_url);
    println!("gRPC address: {}", grpc_addr);

    // Scylla session
    println!("Running database migrations...");
    run_database_migrations(&scylla_host).await?;

    let profile = ExecutionProfile::builder()
        .consistency(Consistency::One)
        .build();

    println!("Connecting to ScyllaDB...");
    let session: Session = SessionBuilder::new()
        .known_node(&scylla_host)
        .default_execution_profile_handle(profile.into_handle())
        .build()
        .await?;
    let session_arc = Arc::new(session);

    // RabbitMQ connections
    println!("Connecting to RabbitMQ...");
    let connection = Connection::connect(&rabbitmq_url, ConnectionProperties::default()).await?;
    let queue_name = "direct_messages".to_string();
    let publisher = Arc::new(MessagePublisher::new(&connection, queue_name.clone()).await?);

    // Create the gRPC service
    println!("Creating gRPC service...");
    println!("Creating gRPC service...");
    let chat_service = ChatServiceImpl::new(Arc::clone(&session_arc), publisher);
    let service = ChatServiceServer::new(chat_service);
    let addr = grpc_addr.parse()?;

    let (health_reporter, health_service) = health_reporter();
    health_reporter
        .set_serving::<ChatServiceServer<ChatServiceImpl>>()
        .await;

    println!("ChatService gRPC server listening on {}", addr);

    // Start the gRPC server
    Server::builder()
        .add_service(health_service)
        .add_service(service)
        .serve(addr)
        .await?;

    Ok(())
}

use crate::chat_services::ChatServiceImpl;
use scylla::client::session_builder::SessionBuilder;
use scylla::client::{execution_profile::ExecutionProfile, session::Session};
use scylla::statement::prepared::PreparedStatement;
use std::env;
use std::error::Error;
use std::sync::Arc;
use tonic::transport::Server;
use tonic_health::server::health_reporter;
mod chat_services;
mod queries;
use crate::chat_service::chat_service_server::ChatServiceServer;
use crate::migrations::migration::run_database_migrations;
pub mod chat_service {
    tonic::include_proto!("chat_service");
}
mod migrations;
mod utils;

pub struct Queries {
    pub q_fetch_after_message_id: PreparedStatement,
}

pub async fn prepare_queries(
    session: &Session,
) -> Result<Arc<Queries>, Box<dyn std::error::Error>> {
    let query_text = r#"
        SELECT conversation_id, message_id, sender_id, recipient_id, message_text, created_at 
        FROM affinity.direct_messages 
        WHERE project_id = ? AND conversation_id = ? AND message_id > ?
    "#;

    let q_fetch_after_message_id = session.prepare(query_text).await?;

    Ok(Arc::new(Queries {
        q_fetch_after_message_id,
    }))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let scylla_host = env::var("SCYLLA_HOST").unwrap_or_else(|_| "127.0.0.1:9042".to_string());

    let grpc_addr = env::var("GRPC_ADDR")?;

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("error".parse().unwrap()), // deepest log level
        )
        .with_target(true)
        .with_line_number(true)
        .init();

    // Scylla session
    println!("Running database migrations...");
    run_database_migrations(&scylla_host).await?;

    let profile = ExecutionProfile::builder()
        .consistency(crate::utils::write_consistency())
        .build();

    let session: Session = SessionBuilder::new()
        .known_node(&scylla_host)
        .default_execution_profile_handle(profile.into_handle())
        .build()
        .await?;
    let session_arc = Arc::new(session);
    let queries = prepare_queries(&session_arc).await?;

    let chat_service = ChatServiceImpl::new(Arc::clone(&session_arc), Arc::clone(&queries));
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

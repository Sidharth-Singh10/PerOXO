use std::error::Error;
use std::sync::Arc;

use scylla::client::execution_profile::ExecutionProfile;
use scylla::client::session::Session;
use scylla::client::session_builder::SessionBuilder;
use scylla::statement::Consistency;

pub struct DatabaseMigrations {
    session: Arc<Session>,
}

impl DatabaseMigrations {
    pub async fn new(node: &str) -> Result<Self, Box<dyn Error>> {
        let profile = ExecutionProfile::builder()
            .consistency(Consistency::One)
            .build();

        let session: Session = SessionBuilder::new()
            .known_node(node)
            .default_execution_profile_handle(profile.into_handle())
            .build()
            .await?;

        Ok(Self {
            session: Arc::new(session),
        })
    }

    pub async fn run_migrations(&self) -> Result<(), Box<dyn Error>> {
        println!("Starting database migrations...");

        // Step 1: Create keyspace
        self.create_keyspace().await?;

        // Step 2: Use keyspace
        self.use_keyspace().await?;

        // Step 3: Create tables
        self.create_tables().await?;

        println!("Database migrations completed successfully!");
        Ok(())
    }

    async fn create_keyspace(&self) -> Result<(), Box<dyn Error>> {
        let query = r#"
            CREATE KEYSPACE IF NOT EXISTS affinity
            WITH REPLICATION = {
                'class': 'SimpleStrategy',
                'replication_factor': 1
            }
        "#;

        println!("Creating keyspace 'affinity'...");
        let prepared = self.session.prepare(query).await?;
        self.session.execute_unpaged(&prepared, &[]).await?;
        println!("Keyspace 'affinity' created successfully");
        Ok(())
    }

    async fn use_keyspace(&self) -> Result<(), Box<dyn Error>> {
        let query = "USE affinity";

        println!("Switching to keyspace 'affinity'...");
        let prepared = self.session.prepare(query).await?;
        self.session.execute_unpaged(&prepared, &[]).await?;
        println!("Now using keyspace 'affinity'");
        Ok(())
    }

    async fn create_tables(&self) -> Result<(), Box<dyn Error>> {
        // Create direct_messages table
        self.create_direct_messages_table().await?;

        // Create user_conversations table
        self.create_user_conversations_table().await?;

        Ok(())
    }

    async fn create_direct_messages_table(&self) -> Result<(), Box<dyn Error>> {
        let query = r#"
            CREATE TABLE IF NOT EXISTS direct_messages (
                conversation_id text,      
                message_id uuid,       
                sender_id int,            
                recipient_id int,         
                message_text text,
                created_at timestamp,
                PRIMARY KEY ((conversation_id), message_id)
            ) WITH CLUSTERING ORDER BY (message_id ASC)
        "#;

        println!("Creating table 'direct_messages'...");
        let prepared = self.session.prepare(query).await?;
        self.session.execute_unpaged(&prepared, &[]).await?;
        println!("Table 'direct_messages' created successfully");
        Ok(())
    }

    async fn create_user_conversations_table(&self) -> Result<(), Box<dyn Error>> {
        let query = r#"
            CREATE TABLE IF NOT EXISTS user_conversations (
                user_id uuid,
                conversation_id text,
                last_message timestamp,
                PRIMARY KEY (user_id, conversation_id)
            )
        "#;

        println!("Creating table 'user_conversations'...");
        let prepared = self.session.prepare(query).await?;
        self.session.execute_unpaged(&prepared, &[]).await?;
        println!("Table 'user_conversations' created successfully");
        Ok(())
    }

    pub fn get_session(&self) -> Arc<Session> {
        Arc::clone(&self.session)
    }
}

// Helper function to run migrations from main or other modules
pub async fn run_database_migrations(node: &str) -> Result<Arc<Session>, Box<dyn Error>> {
    let migrations = DatabaseMigrations::new(node).await?;
    migrations.run_migrations().await?;
    Ok(migrations.get_session())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_migrations() {
        // Note: This test requires a running Scylla instance
        // Uncomment and modify as needed for your testing environment

        let result = run_database_migrations("127.0.0.1:9042").await;
        assert!(result.is_ok());
    }
}

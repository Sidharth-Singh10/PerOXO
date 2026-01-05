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

        self.create_keyspace().await?;

        self.use_keyspace().await?;

        self.create_tables().await?;

        println!("Database migrations completed successfully!");
        Ok(())
    }

    async fn create_keyspace(&self) -> Result<(), Box<dyn Error>> {
        let query = r#"
            CREATE KEYSPACE IF NOT EXISTS affinity2
            WITH REPLICATION = {
                'class': 'SimpleStrategy',
                'replication_factor': 1
            }
        "#;

        println!("Creating keyspace 'affinity'...");
        let prepared = self.session.prepare(query).await?;
        self.session.execute_unpaged(&prepared, &[]).await?;
        println!("Keyspace 'affinity2' created successfully");
        Ok(())
    }

    async fn use_keyspace(&self) -> Result<(), Box<dyn Error>> {
        let query = "USE affinity2";

        println!("Switching to keyspace 'affinity2'...");
        let prepared = self.session.prepare(query).await?;
        self.session.execute_unpaged(&prepared, &[]).await?;
        println!("Now using keyspace 'affinity2'");
        Ok(())
    }

    async fn create_tables(&self) -> Result<(), Box<dyn Error>> {
        self.create_projects_table().await?;
        self.create_direct_messages_table().await?;
        self.create_user_conversations_table().await?;
        self.create_room_messages_table().await?;
        self.create_project_rooms_table().await?;

        Ok(())
    }

    async fn create_projects_table(&self) -> Result<(), Box<dyn Error>> {
        let query = r#"
            CREATE TABLE IF NOT EXISTS projects (
                project_id text PRIMARY KEY,
                name text,
                created_at timestamp
            )
        "#;

        println!("Creating table 'projects'...");
        let prepared = self.session.prepare(query).await?;
        self.session.execute_unpaged(&prepared, &[]).await?;
        println!("Table 'projects' created successfully");
        Ok(())
    }

    async fn create_direct_messages_table(&self) -> Result<(), Box<dyn Error>> {
        // Partition key: (project_id, conversation_id)
        // Clustering key: message_id (timeuuid)
        let query = r#"
            CREATE TABLE IF NOT EXISTS direct_messages (
                project_id text,
                conversation_id text,
                message_id timeuuid,
                sender_id text,
                recipient_id text,
                message_text text,
                created_at timestamp,
                PRIMARY KEY ((project_id, conversation_id), message_id)
            ) WITH CLUSTERING ORDER BY (message_id ASC)
        "#;

        println!("Creating table 'direct_messages'...");
        let prepared = self.session.prepare(query).await?;
        self.session.execute_unpaged(&prepared, &[]).await?;
        println!("Table 'direct_messages' created successfully");
        Ok(())
    }

    async fn create_user_conversations_table(&self) -> Result<(), Box<dyn Error>> {
        // Partition key: (project_id, user_id)
        // Clustering key: conversation_id
        let query = r#"
            CREATE TABLE IF NOT EXISTS user_conversations (
                project_id text,
                user_id text,
                conversation_id text,
                last_message timestamp,
                PRIMARY KEY ((project_id, user_id), conversation_id)
            )
        "#;

        println!("Creating table 'user_conversations'...");
        let prepared = self.session.prepare(query).await?;
        self.session.execute_unpaged(&prepared, &[]).await?;
        println!("Table 'user_conversations' created successfully");
        Ok(())
    }

    async fn create_room_messages_table(&self) -> Result<(), Box<dyn Error>> {
        // Partition key: (project_id, room_id)
        // Clustering key: message_id (timeuuid)
        let query = r#"
            CREATE TABLE IF NOT EXISTS room_messages (
                project_id text,
                room_id text,
                message_id timeuuid,
                sender_id text,
                content text,
                created_at timestamp,
                PRIMARY KEY ((project_id, room_id), message_id)
            ) WITH CLUSTERING ORDER BY (message_id ASC)
        "#;

        println!("Creating table 'room_messages'...");
        let prepared = self.session.prepare(query).await?;
        self.session.execute_unpaged(&prepared, &[]).await?;
        println!("Table 'room_messages' created successfully");
        Ok(())
    }

    async fn create_project_rooms_table(&self) -> Result<(), Box<dyn Error>> {
        // Partition key: (project_id)
        // Clustering key: room_id
        let query = r#"
            CREATE TABLE IF NOT EXISTS project_rooms (
                project_id text,
                room_id text,
                last_activity timestamp,
                PRIMARY KEY ((project_id), room_id)
            ) WITH CLUSTERING ORDER BY (room_id ASC)
        "#;

        println!("Creating table 'project_rooms'...");
        let prepared = self.session.prepare(query).await?;
        self.session.execute_unpaged(&prepared, &[]).await?;
        println!("Table 'project_rooms' created successfully");
        Ok(())
    }

    pub fn get_session(&self) -> Arc<Session> {
        Arc::clone(&self.session)
    }
}

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
        // This test requires a running Scylla instance at 127.0.0.1:9042
        let result = run_database_migrations("127.0.0.1:9042").await;

        if let Err(e) = &result {
            eprintln!("Migration failed: {:?}", e);
        }

        assert!(result.is_ok());
    }
}

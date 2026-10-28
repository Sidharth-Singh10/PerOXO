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
        // Replication factor defaults to 3 for durability; single-node dev
        // clusters should set SCYLLA_REPLICATION_FACTOR=1. Note this only
        // affects new keyspaces — ALTER is needed to change an existing one.
        let replication_factor = std::env::var("SCYLLA_REPLICATION_FACTOR")
            .ok()
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(3);

        let query = format!(
            "CREATE KEYSPACE IF NOT EXISTS affinity \
             WITH REPLICATION = {{ 'class': 'SimpleStrategy', 'replication_factor': {} }}",
            replication_factor
        );

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
        self.create_projects_table().await?;
        self.create_direct_messages_table().await?;
        self.create_user_conversations_table().await?;
        self.create_dm_lookup_table().await?;
        self.create_conversation_members_table().await?;
        self.create_room_messages_table().await?;
        self.create_project_rooms_table().await?;

        Ok(())
    }

    /// Membership index for DMs.
    ///
    /// `dm_lookup` is keyed by `(project_id, user_id_1, user_id_2)`, so it can
    /// answer "what is the conversation for this pair" but not "is this caller
    /// a participant of this conversation". The read path needs the latter to
    /// stop one tenant member from reading another pair's history with a
    /// guessed conversation id, so membership is materialized here.
    async fn create_conversation_members_table(&self) -> Result<(), Box<dyn Error>> {
        let query = r#"
            CREATE TABLE IF NOT EXISTS conversation_members (
                project_id text,
                conversation_id text,
                user_id text,
                PRIMARY KEY ((project_id, conversation_id), user_id)
            )
        "#;

        println!("Creating table 'conversation_members'...");
        let prepared = self.session.prepare(query).await?;
        self.session.execute_unpaged(&prepared, &[]).await?;
        println!("Table 'conversation_members' created successfully");
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

    async fn create_dm_lookup_table(&self) -> Result<(), Box<dyn Error>> {
        let query = r#"
            CREATE TABLE IF NOT EXISTS dm_lookup (
                project_id text,
                user_id_1 text,
                user_id_2 text,
                conversation_id text,
                created_at timestamp,
                PRIMARY KEY ((project_id, user_id_1, user_id_2))
            )
        "#;

        println!("Creating table 'dm_lookup'...");
        let prepared = self.session.prepare(query).await?;
        self.session.execute_unpaged(&prepared, &[]).await?;
        println!("Table 'dm_lookup' created successfully");
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

    /// One-off backfill of `conversation_members` from the existing
    /// `dm_lookup` rows.
    ///
    /// Enabled with `BACKFILL_CONVERSATION_MEMBERS=1` on startup. It is
    /// idempotent: inserting an existing `(project_id, conversation_id,
    /// user_id)` row is a no-op upsert. Run it once after deploying the
    /// membership checks, before enabling them for existing deployments, so
    /// that conversations created before the index existed stay readable by
    /// their participants.
    pub async fn backfill_conversation_members(&self) -> Result<(), Box<dyn Error>> {
        println!("Backfilling 'conversation_members' from 'dm_lookup'...");

        let rows = self
            .session
            .query_unpaged(
                "SELECT project_id, user_id_1, user_id_2, conversation_id FROM affinity.dm_lookup",
                &[],
            )
            .await?;

        let rows_result = rows.into_rows_result()?;
        let typed_rows = rows_result.rows::<(String, String, String, Option<String>)>()?;

        let insert = self
            .session
            .prepare(
                "INSERT INTO affinity.conversation_members \
                 (project_id, conversation_id, user_id) VALUES (?, ?, ?)",
            )
            .await?;

        let mut pairs = 0u64;
        for row in typed_rows {
            let (project_id, user_id_1, user_id_2, conversation_id) = row?;

            let Some(conversation_id) = conversation_id else {
                continue;
            };

            for user_id in [&user_id_1, &user_id_2] {
                self.session
                    .execute_unpaged(&insert, (&project_id, &conversation_id, user_id))
                    .await?;
            }
            pairs += 1;
        }

        println!("Backfilled membership for {pairs} conversation pairs");
        Ok(())
    }
}

pub async fn run_database_migrations(node: &str) -> Result<Arc<Session>, Box<dyn Error>> {
    let migrations = DatabaseMigrations::new(node).await?;

    // 2. Run the migrations
    migrations.run_migrations().await?;

    if std::env::var("BACKFILL_CONVERSATION_MEMBERS")
        .ok()
        .as_deref()
        == Some("1")
    {
        migrations.backfill_conversation_members().await?;
    }

    println!("Initializing application session with keyspace 'affinity'...");

    let app_session = SessionBuilder::new()
        .known_node(node)
        .use_keyspace("affinity", true)
        .build()
        .await?;

    Ok(Arc::new(app_session))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "requires a running Scylla node on 127.0.0.1:9042"]
    async fn test_migrations() {
        // This test requires a running Scylla instance at 127.0.0.1:9042
        let result = run_database_migrations("127.0.0.1:9042").await;

        if let Err(e) = &result {
            eprintln!("Migration failed: {:?}", e);
        }

        assert!(result.is_ok());
    }
}

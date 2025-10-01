pub struct MongoDbConfig {
    pub connection_url: String,
    pub database_name: String,
}

impl MongoDbConfig {
    pub fn new(connection_url: impl Into<String>) -> Self {
        Self {
            connection_url: connection_url.into(),
            database_name: "affinity".to_string(),
        }
    }

    pub fn with_database_name(mut self, database_name: impl Into<String>) -> Self {
        self.database_name = database_name.into();
        self
    }
}

use serde::{Deserialize, Serialize};

use crate::UserToken;

#[derive(Clone, Debug, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct TenantUserId {
    pub project_id: String,
    pub user_id: String,
}

impl TenantUserId {
    pub fn new(project_id: String, user_id: String) -> Self {
        Self {
            project_id,
            user_id,
        }
    }

    pub fn from_token(token: &UserToken) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            project_id: token.project_id.clone(),
            user_id: token.user_id.clone(),
        })
    }
}

impl std::fmt::Display for TenantUserId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.project_id, self.user_id)
    }
}

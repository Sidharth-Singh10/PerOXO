use nanoid::nanoid;
use rand::{Rng, distributions::Alphanumeric};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct TenantKeypair {
    secret_api_key: String,
    project_id: String,
}

impl TenantKeypair {
    pub fn new() -> Self {
        let secret_api_key = Self::generate_secret_api_key();
        let project_id = Self::generate_project_id();

        TenantKeypair {
            secret_api_key,
            project_id,
        }
    }

    pub fn get_secret_api_key(&self) -> &String {
        &self.secret_api_key
    }
    pub fn get_project_id(&self) -> &String {
        &self.project_id
    }

    fn generate_secret_api_key() -> String {
        let random_segment: String = rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(40)
            .map(char::from)
            .collect();

        format!("peroxo_{}", random_segment)
    }

    fn generate_project_id() -> String {
        format!("peroxo_pj_{}", nanoid!())
    }
}

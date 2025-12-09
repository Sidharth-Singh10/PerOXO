use rand::Rng;
use rand::distributions::Alphanumeric;
use redis::Commands;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Serialize, Deserialize)]
pub struct UserToken {
    pub project_id: String,
    pub user_id: String,
    pub expires_at: u64,
}

fn generate_token() -> String {
    let rand_string: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(24)
        .map(char::from)
        .collect();

    format!("pxtok_{}", rand_string)
}

pub async fn store_user_token(
    redis_client: &redis::Client,
    project_id: &str,
    user_id: &str,
) -> redis::RedisResult<String> {
    let ttl_secs = 600;

    let token = generate_token();

    let expires_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + ttl_secs;

    let payload = UserToken {
        project_id: project_id.to_string(),
        user_id: user_id.to_string(),
        expires_at,
    };

    let json_value = serde_json::to_string(&payload).unwrap();

    let mut con = redis_client.get_connection()?;
    let key = &token;

    let _: () = con.set_ex(key, json_value, ttl_secs)?;

    Ok(token)
}

pub async fn verify_user_token(
    redis_client: &redis::Client,
    token: &str,
) -> Result<Option<UserToken>, Box<dyn std::error::Error>> {
    if !token.starts_with("pxtok_") {
        return Ok(None);
    }

    let mut con = redis_client.get_connection()?;

    let data: Option<String> = con.get(token)?;

    let json = match data {
        Some(v) => v,
        None => return Ok(None),
    };

    let parsed: UserToken = match serde_json::from_str(&json) {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    if now > parsed.expires_at {
        return Ok(None);
    }

    Ok(Some(parsed))
}

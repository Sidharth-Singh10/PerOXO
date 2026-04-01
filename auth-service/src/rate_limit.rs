use redis::Commands;
use tracing::{instrument, warn};

#[derive(Debug)]
pub enum RateLimitError {
    Exceeded,
    RedisError(redis::RedisError),
}

impl From<redis::RedisError> for RateLimitError {
    fn from(e: redis::RedisError) -> Self {
        RateLimitError::RedisError(e)
    }
}

const MAX_REQUESTS: i64 = 3;
const WINDOW_SECS: i64 = 3600;

/// Fixed-window rate limiter backed by Redis.
/// Allows `MAX_REQUESTS` calls per `WINDOW_SECS` window, keyed by Google `sub`.
#[instrument(skip(redis_client))]
pub fn check_rate_limit(
    redis_client: &redis::Client,
    google_sub: &str,
) -> Result<(), RateLimitError> {
    let key = format!("rl:gen_tenant:{}", google_sub);
    let mut con = redis_client.get_connection()?;

    let count: i64 = con.incr(&key, 1)?;
    if count == 1 {
        let _: bool = con.expire(&key, WINDOW_SECS)?;
    }

    if count > MAX_REQUESTS {
        warn!(google_sub = %google_sub, count = %count, "rate limit exceeded for generate-tenant");
        return Err(RateLimitError::Exceeded);
    }

    Ok(())
}

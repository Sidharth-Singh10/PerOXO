use redis::aio::ConnectionManager;
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

// Atomic fixed-window limiter: INCR and first-window EXPIRE happen in a
// single Lua round trip, so a crash between INCR and EXPIRE can never
// leave the key without a TTL (permanent lockout).
const INCR_AND_EXPIRE_SCRIPT: &str = r#"
    local count = redis.call('INCR', KEYS[1])
    if count == 1 then
        redis.call('EXPIRE', KEYS[1], ARGV[1])
    end
    return count
"#;

/// Fixed-window rate limiter backed by Redis.
/// Allows `MAX_REQUESTS` calls per `WINDOW_SECS` window, keyed by Google `sub`.
#[instrument(skip(redis))]
pub async fn check_rate_limit(
    redis: &ConnectionManager,
    google_sub: &str,
) -> Result<(), RateLimitError> {
    let key = format!("rl:gen_tenant:{}", google_sub);

    let count: i64 = redis::Script::new(INCR_AND_EXPIRE_SCRIPT)
        .key(key)
        .arg(WINDOW_SECS)
        .invoke_async(&mut redis.clone())
        .await?;

    if count > MAX_REQUESTS {
        warn!(google_sub = %google_sub, count = %count, "rate limit exceeded for generate-tenant");
        return Err(RateLimitError::Exceeded);
    }

    Ok(())
}
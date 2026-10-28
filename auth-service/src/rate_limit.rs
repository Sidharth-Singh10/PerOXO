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

/// Fixed window definition for one endpoint.
#[derive(Debug, Clone, Copy)]
pub struct Limit {
    pub bucket: &'static str,
    pub max_requests: i64,
    pub window_secs: i64,
}

/// Tenant creation is expensive and user-visible: 3 per hour per Google account.
pub const GENERATE_TENANT: Limit = Limit {
    bucket: "gen_tenant",
    max_requests: 3,
    window_secs: 3600,
};

/// Secret-key authentication: bound guessing attempts per (IP, project).
pub const GENERATE_USER_TOKEN: Limit = Limit {
    bucket: "gen_user_token",
    max_requests: 10,
    window_secs: 60,
};

/// Token lookups are unauthenticated by nature; bound enumeration per IP.
pub const VERIFY_USER_TOKEN: Limit = Limit {
    bucket: "verify_user_token",
    max_requests: 60,
    window_secs: 60,
};

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

/// Fixed-window rate limiter backed by Redis, keyed by `subject`.
///
/// The counter lives in Redis rather than process memory so that the limit is
/// correct with more than one instance behind a load balancer.
#[instrument(skip(redis, subject))]
pub async fn check(
    redis: &ConnectionManager,
    limit: Limit,
    subject: &str,
) -> Result<(), RateLimitError> {
    // Bound the key length: subjects come from request data (project ids, IPs)
    // and must not be able to grow the keyspace without limit.
    let subject = &subject[..subject.len().min(128)];
    let key = format!("rl:{}:{}", limit.bucket, subject);

    let count: i64 = redis::Script::new(INCR_AND_EXPIRE_SCRIPT)
        .key(key)
        .arg(limit.window_secs)
        .invoke_async(&mut redis.clone())
        .await?;

    if count > limit.max_requests {
        warn!(
            bucket = limit.bucket,
            count = count,
            max = limit.max_requests,
            "rate limit exceeded"
        );
        return Err(RateLimitError::Exceeded);
    }

    Ok(())
}

/// Convenience wrapper for tenant creation, keyed by Google `sub`.
#[instrument(skip(redis))]
pub async fn check_rate_limit(
    redis: &ConnectionManager,
    google_sub: &str,
) -> Result<(), RateLimitError> {
    check(redis, GENERATE_TENANT, google_sub).await
}

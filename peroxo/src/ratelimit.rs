//! Per-connection message throttle.
//!
//! A single authenticated client can otherwise push frames as fast as the
//! socket accepts them: every message is parsed, routed, and persisted, so the
//! server's cost is set by the fastest sender rather than by capacity. A token
//! bucket bounds the sustained rate while still allowing short bursts.

use std::time::{Duration, Instant};

/// Sustained messages per second per connection.
pub const REFILL_PER_SEC: f64 = 30.0;

/// Burst allowance: a client may send this many messages back to back before
/// the sustained rate kicks in.
pub const BURST_CAPACITY: f64 = 60.0;

pub struct TokenBucket {
    tokens: f64,
    capacity: f64,
    refill_per_sec: f64,
    last_refill: Instant,
}

impl TokenBucket {
    pub fn new() -> Self {
        Self::with_rate(REFILL_PER_SEC, BURST_CAPACITY)
    }

    pub fn with_rate(refill_per_sec: f64, capacity: f64) -> Self {
        Self {
            tokens: capacity,
            capacity,
            refill_per_sec,
            last_refill: Instant::now(),
        }
    }

    /// Consumes one token if available. Returns false when the caller is over
    /// budget and should be disconnected.
    pub fn try_consume(&mut self) -> bool {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill);
        self.last_refill = now;

        self.tokens =
            (self.tokens + elapsed.as_secs_f64() * self.refill_per_sec).min(self.capacity);

        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

impl Default for TokenBucket {
    fn default() -> Self {
        Self::new()
    }
}

/// Grace period used when closing an abusive connection, so the close frame has
/// a chance to flush.
pub const CLOSE_GRACE: Duration = Duration::from_millis(100);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_a_burst_then_denies() {
        let mut bucket = TokenBucket::with_rate(10.0, 3.0);

        assert!(bucket.try_consume());
        assert!(bucket.try_consume());
        assert!(bucket.try_consume());
        assert!(!bucket.try_consume());
    }

    #[test]
    fn refills_over_time() {
        let mut bucket = TokenBucket::with_rate(1_000.0, 1.0);
        assert!(bucket.try_consume());
        assert!(!bucket.try_consume());

        std::thread::sleep(Duration::from_millis(5));
        assert!(bucket.try_consume());
    }
}

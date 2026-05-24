//! Per-client, per-message-type token-bucket rate limiter.
//!
//! Phase 0.8 scaffold: only the bucket mechanics + a registry keyed by
//! `(client_id, message_kind)`. The ws handler in Phase 1 wires this in
//! with permissive defaults; Phase 6.5 tightens the limits per
//! `protocol.md` §rate-limits.
//!
//! Time is injected through a [`Clock`] so unit tests are deterministic
//! and never depend on `Instant::now()` wall-clock drift.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::DashMap;

/// Pluggable monotonic clock — production uses [`SystemClock`], tests use
/// [`TestClock`] to advance time explicitly.
pub trait Clock: Send + Sync + 'static {
    /// Monotonic nanoseconds since an arbitrary epoch.
    fn now_nanos(&self) -> u64;
}

#[derive(Debug)]
pub struct SystemClock {
    start: Instant,
}

impl Default for SystemClock {
    fn default() -> Self {
        Self {
            start: Instant::now(),
        }
    }
}

impl Clock for SystemClock {
    fn now_nanos(&self) -> u64 {
        // Safe: Duration::as_nanos() on a monotonic Instant fits in u64 for
        // ~584 years of uptime.
        self.start.elapsed().as_nanos() as u64
    }
}

#[derive(Debug, Default)]
pub struct TestClock(AtomicU64);

impl TestClock {
    pub fn new() -> Self {
        Self(AtomicU64::new(0))
    }
    pub fn advance(&self, d: Duration) {
        self.0.fetch_add(d.as_nanos() as u64, Ordering::Relaxed);
    }
}

impl Clock for TestClock {
    fn now_nanos(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }
}

/// Configuration for a single bucket: how many tokens fit, and how many
/// tokens refill per second.
#[derive(Debug, Clone, Copy)]
pub struct Quota {
    pub capacity: f64,
    pub refill_per_sec: f64,
}

impl Quota {
    pub const fn per_second(rate: f64) -> Self {
        Self {
            capacity: rate,
            refill_per_sec: rate,
        }
    }

    pub const fn per_minute(rate: f64) -> Self {
        Self {
            capacity: rate,
            refill_per_sec: rate / 60.0,
        }
    }
}

#[derive(Debug)]
struct Bucket {
    tokens: f64,
    last_nanos: u64,
    quota: Quota,
}

impl Bucket {
    fn new(quota: Quota, now_nanos: u64) -> Self {
        Self {
            tokens: quota.capacity,
            last_nanos: now_nanos,
            quota,
        }
    }

    fn refill(&mut self, now_nanos: u64) {
        if now_nanos <= self.last_nanos {
            return;
        }
        let elapsed = (now_nanos - self.last_nanos) as f64 / 1_000_000_000.0;
        self.tokens = (self.tokens + elapsed * self.quota.refill_per_sec).min(self.quota.capacity);
        self.last_nanos = now_nanos;
    }

    fn try_take(&mut self, now_nanos: u64) -> bool {
        self.refill(now_nanos);
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

/// Registry of buckets keyed by `(client_id, message_kind)`.
pub struct RateLimiter<C: Clock = SystemClock> {
    clock: Arc<C>,
    buckets: DashMap<(String, &'static str), Bucket>,
}

impl<C: Clock> Clone for RateLimiter<C> {
    fn clone(&self) -> Self {
        Self {
            clock: self.clock.clone(),
            buckets: DashMap::new(),
        }
    }
}

impl<C: Clock> RateLimiter<C> {
    pub fn new(clock: Arc<C>) -> Self {
        Self {
            clock,
            buckets: DashMap::new(),
        }
    }

    /// Returns `true` if the call is allowed (and consumes one token) or
    /// `false` if the bucket is empty (the caller should drop the message
    /// and, per `protocol.md`, may emit `Error{code:"rate_limit"}`).
    pub fn check(&self, client_id: &str, kind: &'static str, quota: Quota) -> bool {
        let key = (client_id.to_string(), kind);
        let mut entry = self
            .buckets
            .entry(key)
            .or_insert_with(|| Bucket::new(quota, self.clock.now_nanos()));
        let now = self.clock.now_nanos();
        entry.try_take(now)
    }

    /// Drop all buckets for a client (called on disconnect).
    pub fn forget_client(&self, client_id: &str) {
        self.buckets.retain(|(c, _), _| c != client_id);
    }
}

impl RateLimiter<SystemClock> {
    pub fn with_system_clock() -> Self {
        Self::new(Arc::new(SystemClock::default()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limiter() -> (Arc<TestClock>, RateLimiter<TestClock>) {
        let clock = Arc::new(TestClock::new());
        let rl = RateLimiter::new(clock.clone());
        (clock, rl)
    }

    #[test]
    fn allows_up_to_capacity_then_drops() {
        let (_clk, rl) = limiter();
        let q = Quota::per_second(5.0);
        for _ in 0..5 {
            assert!(rl.check("c1", "Cursor", q));
        }
        assert!(!rl.check("c1", "Cursor", q), "6th call must be dropped");
    }

    #[test]
    fn refills_at_configured_rate() {
        let (clk, rl) = limiter();
        let q = Quota::per_second(2.0);
        assert!(rl.check("c1", "Click", q));
        assert!(rl.check("c1", "Click", q));
        assert!(!rl.check("c1", "Click", q));
        clk.advance(Duration::from_millis(500));
        // 0.5s @ 2/s = 1 token refilled
        assert!(rl.check("c1", "Click", q));
        assert!(!rl.check("c1", "Click", q));
    }

    #[test]
    fn refill_caps_at_capacity() {
        let (clk, rl) = limiter();
        let q = Quota::per_second(3.0);
        for _ in 0..3 {
            assert!(rl.check("c1", "k", q));
        }
        clk.advance(Duration::from_secs(60));
        // Long idle period must not let the bucket accumulate above capacity.
        for _ in 0..3 {
            assert!(rl.check("c1", "k", q));
        }
        assert!(!rl.check("c1", "k", q));
    }

    #[test]
    fn clients_and_kinds_are_independent() {
        let (_clk, rl) = limiter();
        let q = Quota::per_second(1.0);
        assert!(rl.check("a", "X", q));
        assert!(!rl.check("a", "X", q));
        assert!(rl.check("b", "X", q));
        assert!(rl.check("a", "Y", q));
    }

    #[test]
    fn per_minute_quota_refills_slowly() {
        let (clk, rl) = limiter();
        let q = Quota::per_minute(6.0);
        for _ in 0..6 {
            assert!(rl.check("c1", "SubmitQuestion", q));
        }
        assert!(!rl.check("c1", "SubmitQuestion", q));
        clk.advance(Duration::from_secs(10));
        // 10s of a 6/min refill = 1 token.
        assert!(rl.check("c1", "SubmitQuestion", q));
        assert!(!rl.check("c1", "SubmitQuestion", q));
    }

    #[test]
    fn forget_client_resets_buckets() {
        let (_clk, rl) = limiter();
        let q = Quota::per_second(1.0);
        assert!(rl.check("c1", "X", q));
        assert!(!rl.check("c1", "X", q));
        rl.forget_client("c1");
        assert!(rl.check("c1", "X", q));
    }
}

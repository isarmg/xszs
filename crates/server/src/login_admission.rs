use crate::error::AppError;
use std::{
    collections::HashMap,
    hash::Hash,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use uuid::Uuid;
pub(crate) const LOGIN_BODY_LIMIT_BYTES: usize = 4 * 1024;
const INSTANCE_BURST: u32 = 8;
const INSTANCE_REFILL_INTERVAL: Duration = Duration::from_secs(15);
const INSTANCE_ENTRY_CAPACITY: usize = 4096;
const RATE_ENTRY_TTL: Duration = Duration::from_secs(15 * 60);
#[derive(Clone)]
pub(crate) struct LoginAdmission {
    rates: Arc<Mutex<BoundedBuckets<Uuid>>>,
    work: Arc<Semaphore>,
}
impl Default for LoginAdmission {
    fn default() -> Self {
        Self::new(
            BucketPolicy::new(INSTANCE_BURST, INSTANCE_REFILL_INTERVAL),
            INSTANCE_ENTRY_CAPACITY,
            RATE_ENTRY_TTL,
        )
    }
}
impl LoginAdmission {
    fn new(policy: BucketPolicy, capacity: usize, ttl: Duration) -> Self {
        Self {
            rates: Arc::new(Mutex::new(BoundedBuckets::new(policy, capacity, ttl))),
            work: Arc::new(Semaphore::new(16)),
        }
    }
    pub(crate) fn acquire_work(&self) -> Result<OwnedSemaphorePermit, AppError> {
        self.work.clone().try_acquire_owned().map_err(|_| {
            AppError::too_many_requests_with_message(1, "pairing work capacity is busy")
        })
    }
    pub(crate) fn check_instance(&self, verified_instance: Uuid) -> Result<(), AppError> {
        self.rates
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .check_at(verified_instance, Instant::now())
            .map_err(rate_limited)
    }
}
fn rate_limited(delay: Duration) -> AppError {
    AppError::too_many_requests(retry_after_seconds(delay))
}

fn retry_after_seconds(delay: Duration) -> u64 {
    delay
        .as_secs()
        .saturating_add(u64::from(delay.subsec_nanos() != 0))
        .max(1)
}

#[derive(Clone, Copy)]
struct BucketPolicy {
    burst: u32,
    refill_interval: Duration,
}

impl BucketPolicy {
    fn new(burst: u32, refill_interval: Duration) -> Self {
        assert!(burst > 0);
        assert!(!refill_interval.is_zero());
        Self {
            burst,
            refill_interval,
        }
    }
}

#[derive(Clone, Copy)]
struct Bucket {
    tokens: f64,
    last_refill: Instant,
    last_seen: Instant,
}

impl Bucket {
    fn new(policy: BucketPolicy, now: Instant) -> Self {
        Self {
            tokens: f64::from(policy.burst),
            last_refill: now,
            last_seen: now,
        }
    }

    fn tokens_at(self, policy: BucketPolicy, now: Instant) -> f64 {
        let elapsed = now.saturating_duration_since(self.last_refill);
        (self.tokens + elapsed.as_secs_f64() / policy.refill_interval.as_secs_f64())
            .min(f64::from(policy.burst))
    }

    fn refill(&mut self, policy: BucketPolicy, now: Instant) {
        self.tokens = self.tokens_at(policy, now);
        self.last_refill = now;
        self.last_seen = now;
    }

    fn delay_until_tokens(self, policy: BucketPolicy, wanted: f64, now: Instant) -> Duration {
        let missing = (wanted - self.tokens_at(policy, now)).max(0.0);
        Duration::from_secs_f64(missing * policy.refill_interval.as_secs_f64())
    }
}

struct BoundedBuckets<K> {
    entries: HashMap<K, Bucket>,
    policy: BucketPolicy,
    capacity: usize,
    entry_ttl: Duration,
}

impl<K> BoundedBuckets<K>
where
    K: Clone + Eq + Hash,
{
    fn new(policy: BucketPolicy, capacity: usize, entry_ttl: Duration) -> Self {
        assert!(capacity > 0);
        assert!(!entry_ttl.is_zero());
        Self {
            entries: HashMap::new(),
            policy,
            capacity,
            entry_ttl,
        }
    }

    fn check_at(&mut self, key: K, now: Instant) -> Result<(), Duration> {
        self.prune_expired(now);
        if !self.entries.contains_key(&key) {
            self.make_room(now)?;
            self.entries
                .insert(key.clone(), Bucket::new(self.policy, now));
        }
        let entry = self.entries.get_mut(&key).expect("bucket was inserted");
        entry.refill(self.policy, now);
        if entry.tokens < 1.0 {
            return Err(entry.delay_until_tokens(self.policy, 1.0, now));
        }
        entry.tokens -= 1.0;
        Ok(())
    }

    fn prune_expired(&mut self, now: Instant) {
        self.entries
            .retain(|_, entry| now.saturating_duration_since(entry.last_seen) < self.entry_ttl);
    }

    fn make_room(&mut self, now: Instant) -> Result<(), Duration> {
        if self.entries.len() < self.capacity {
            return Ok(());
        }
        let evictable = self
            .entries
            .iter()
            .filter(|(_, entry)| entry.tokens_at(self.policy, now) >= f64::from(self.policy.burst))
            .min_by_key(|(_, entry)| entry.last_seen)
            .map(|(key, _)| key.clone());
        if let Some(key) = evictable {
            self.entries.remove(&key);
            return Ok(());
        }
        let retry = self
            .entries
            .values()
            .map(|entry| {
                let until_full =
                    entry.delay_until_tokens(self.policy, f64::from(self.policy.burst), now);
                let until_expiry = self
                    .entry_ttl
                    .saturating_sub(now.saturating_duration_since(entry.last_seen));
                until_full.min(until_expiry)
            })
            .min()
            .unwrap_or(self.entry_ttl);
        Err(retry.max(Duration::from_nanos(1)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn verified_instance_limits_do_not_share_an_address_or_credential_bucket() {
        let admission = LoginAdmission::new(
            BucketPolicy::new(1, Duration::from_secs(60)),
            2,
            Duration::from_secs(300),
        );
        let first = Uuid::new_v4();
        admission.check_instance(first).unwrap();
        assert!(admission.check_instance(first).is_err());
        admission.check_instance(Uuid::new_v4()).unwrap();
        assert!(admission.check_instance(Uuid::new_v4()).is_err());
    }
    #[test]
    fn bounded_buckets_expire_and_only_evict_replenished_entries() {
        let policy = BucketPolicy::new(1, Duration::from_secs(1));
        let ttl = Duration::from_secs(10);
        let mut buckets = BoundedBuckets::new(policy, 2, ttl);
        let now = Instant::now();
        buckets.check_at("first", now).unwrap();
        buckets
            .check_at("second", now + Duration::from_millis(100))
            .unwrap();
        assert!(
            buckets
                .check_at("third", now + Duration::from_millis(200))
                .is_err()
        );
        buckets
            .check_at("third", now + Duration::from_secs(2))
            .unwrap();
        assert!(!buckets.entries.contains_key("first"));
        buckets
            .check_at("after-expiry", now + ttl + Duration::from_secs(3))
            .unwrap();
        assert_eq!(buckets.entries.len(), 1);
    }
}

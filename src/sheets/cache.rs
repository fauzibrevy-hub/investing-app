//! In-memory TTL cache for Sheets reads (MC `_wrap_cached`: 45 s for ledger
//! data, 5 min for category named ranges). Writes call `clear()`.

use std::collections::HashMap;
use std::future::Future;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub struct TtlCache<V> {
    ttl: Duration,
    entries: Mutex<HashMap<String, (Instant, V)>>,
}

impl<V: Clone> TtlCache<V> {
    pub fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            entries: Mutex::new(HashMap::new()),
        }
    }

    pub fn get(&self, key: &str) -> Option<V> {
        let entries = self.entries.lock().expect("cache lock");
        entries
            .get(key)
            .filter(|(at, _)| at.elapsed() < self.ttl)
            .map(|(_, v)| v.clone())
    }

    pub fn put(&self, key: &str, value: V) {
        self.entries
            .lock()
            .expect("cache lock")
            .insert(key.to_string(), (Instant::now(), value));
    }

    pub fn clear(&self) {
        self.entries.lock().expect("cache lock").clear();
    }

    /// Return the cached value or compute and store it. Errors are not cached.
    pub async fn get_or_try<E, F, Fut>(&self, key: &str, f: F) -> Result<V, E>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<V, E>>,
    {
        if let Some(v) = self.get(key) {
            return Ok(v);
        }
        let v = f().await?;
        self.put(key, v.clone());
        Ok(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn caches_until_ttl_and_clear() {
        let cache = TtlCache::new(Duration::from_secs(60));
        let a: Result<i32, ()> = cache.get_or_try("k", || async { Ok(1) }).await;
        let b: Result<i32, ()> = cache.get_or_try("k", || async { Ok(2) }).await;
        assert_eq!((a, b), (Ok(1), Ok(1)));
        cache.clear();
        assert_eq!(cache.get("k"), None);

        let expired = TtlCache::new(Duration::ZERO);
        expired.put("k", 1);
        assert_eq!(expired.get("k"), None);
    }

    #[tokio::test]
    async fn errors_are_not_cached() {
        let cache: TtlCache<i32> = TtlCache::new(Duration::from_secs(60));
        let e: Result<i32, &str> = cache.get_or_try("k", || async { Err("boom") }).await;
        assert!(e.is_err());
        assert_eq!(cache.get("k"), None);
    }
}

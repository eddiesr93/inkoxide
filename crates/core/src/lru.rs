//! A small size-bounded LRU map for the process-wide caches (decoded
//! images, font subsets, converted SVGs and CMYK JPEGs). Entries are few (tens to hundreds), so eviction
//! scans for the oldest instead of keeping an ordered list.
//!
//! WebAssembly memory grows but never shrinks, so there the process-wide
//! caches keep a quarter of their budgets.

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Mutex;

/// What a [`SharedLru`]'s budget is divided by.
const BUDGET_DIVISOR: usize = if cfg!(target_family = "wasm") { 4 } else { 1 };

pub(crate) struct Lru<K, V> {
    entries: HashMap<K, Slot<V>>,
    total: usize,
    budget: usize,
    clock: u64,
}

struct Slot<V> {
    value: V,
    size: usize,
    used: u64,
}

impl<K: Eq + Hash + Clone, V: Clone> Lru<K, V> {
    pub fn new(budget: usize) -> Self {
        Self {
            entries: HashMap::new(),
            total: 0,
            budget,
            clock: 0,
        }
    }

    pub fn get(&mut self, key: &K) -> Option<V> {
        self.clock += 1;
        let now = self.clock;
        self.entries.get_mut(key).map(|s| {
            s.used = now;
            s.value.clone()
        })
    }

    /// Inserts unless `key` is present (the first value wins) or `size`
    /// alone exceeds a quarter of the budget; evicts the least recently
    /// used entries to make room.
    pub fn insert(&mut self, key: K, value: V, size: usize) {
        if size > self.budget / 4 || self.entries.contains_key(&key) {
            return;
        }
        while self.total + size > self.budget {
            let Some(old) = self
                .entries
                .iter()
                .min_by_key(|(_, s)| s.used)
                .map(|(k, _)| k.clone())
            else {
                break;
            };
            if let Some(s) = self.entries.remove(&old) {
                self.total -= s.size;
            }
        }
        self.clock += 1;
        self.total += size;
        self.entries.insert(
            key,
            Slot {
                value,
                size,
                used: self.clock,
            },
        );
    }

    #[cfg(test)]
    pub fn contains(&self, key: &K) -> bool {
        self.entries.contains_key(key)
    }

    #[cfg(test)]
    pub fn total(&self) -> usize {
        self.total
    }
}

/// An [`Lru`] for a `static`: created on first use, behind a lock, with
/// its budget divided by [`BUDGET_DIVISOR`]. A poisoned lock (a panic while
/// it was held) leaves the cache out: lookups miss and inserts are dropped.
pub(crate) struct SharedLru<K, V> {
    budget: usize,
    lru: Mutex<Option<Lru<K, V>>>,
}

impl<K: Eq + Hash + Clone, V: Clone> SharedLru<K, V> {
    pub const fn new(budget: usize) -> Self {
        Self {
            budget: budget / BUDGET_DIVISOR,
            lru: Mutex::new(None),
        }
    }

    /// Drops every entry, and the map that held them.
    pub fn clear(&self) {
        if let Ok(mut guard) = self.lru.lock() {
            *guard = None;
        }
    }

    pub fn get(&self, key: &K) -> Option<V> {
        self.lru.lock().ok()?.as_mut()?.get(key)
    }

    /// See [`Lru::insert`].
    pub fn insert(&self, key: K, value: V, size: usize) {
        if let Ok(mut guard) = self.lru.lock() {
            guard
                .get_or_insert_with(|| Lru::new(self.budget))
                .insert(key, value, size);
        }
    }

    #[cfg(test)]
    pub fn contains(&self, key: &K) -> bool {
        self.lru
            .lock()
            .ok()
            .is_some_and(|g| g.as_ref().is_some_and(|l| l.contains(key)))
    }
}

#[cfg(test)]
mod tests {
    use super::Lru;

    #[test]
    fn evicts_least_recently_used_within_budget() {
        let mut lru = Lru::new(100);
        lru.insert(1, 'a', 20);
        lru.insert(2, 'b', 20);
        lru.insert(3, 'c', 20);
        assert_eq!(lru.get(&1), Some('a'));
        lru.insert(4, 'd', 25);
        lru.insert(5, 'e', 25);
        // 2 was the least recently used.
        assert!(!lru.contains(&2));
        assert!(lru.contains(&1) && lru.contains(&3) && lru.contains(&5));
        assert!(lru.total() <= 100);
        // Too large for the cache: not kept.
        lru.insert(6, 'f', 26);
        assert!(!lru.contains(&6));
        // First value wins.
        lru.insert(1, 'z', 1);
        assert_eq!(lru.get(&1), Some('a'));
    }

    #[test]
    fn shared_caches_clear() {
        let lru = super::SharedLru::new(100);
        lru.insert(1, 'a', 20);
        assert!(lru.contains(&1));
        lru.clear();
        assert!(!lru.contains(&1));
        lru.insert(1, 'b', 20);
        assert_eq!(lru.get(&1), Some('b'));
    }
}

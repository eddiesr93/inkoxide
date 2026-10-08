//! A small size-bounded LRU map for the process-wide caches (decoded
//! images, font subsets). Entries are few (tens to hundreds), so eviction
//! scans for the oldest instead of keeping an ordered list.

use std::collections::HashMap;
use std::hash::Hash;

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
}

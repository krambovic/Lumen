//! Bounded, thread-safe UI log history with snapshot iteration.
//!
//! Port of `windows/xray_fluent/bounded_logs.py`. `LogRing::new(maxlen)` drops the
//! oldest entry on overflow. `dropped` is cumulative; `clear`/`drain` remove
//! entries without resetting this counter.

use std::collections::VecDeque;

use parking_lot::Mutex;

#[derive(Debug)]
struct Inner<T> {
    items: VecDeque<T>,
    dropped: u64,
}

#[derive(Debug)]
pub struct LogRing<T> {
    maxlen: usize,
    inner: Mutex<Inner<T>>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("maxlen must be a positive integer")]
pub struct InvalidMaxlen;

impl<T: Clone> LogRing<T> {
    pub fn new(maxlen: usize) -> Result<Self, InvalidMaxlen> {
        if maxlen < 1 {
            return Err(InvalidMaxlen);
        }
        Ok(Self { maxlen, inner: Mutex::new(Inner { items: VecDeque::with_capacity(maxlen.min(8192)), dropped: 0 }) })
    }

    pub fn with_items(maxlen: usize, items: impl IntoIterator<Item = T>) -> Result<Self, InvalidMaxlen> {
        let ring = Self::new(maxlen)?;
        ring.extend(items);
        Ok(ring)
    }

    pub fn maxlen(&self) -> usize {
        self.maxlen
    }

    pub fn capacity(&self) -> usize {
        self.maxlen
    }

    pub fn dropped(&self) -> u64 {
        self.inner.lock().dropped
    }

    pub fn dropped_count(&self) -> u64 {
        self.dropped()
    }

    pub fn append(&self, item: T) {
        let mut inner = self.inner.lock();
        if inner.items.len() == self.maxlen {
            inner.items.pop_front();
            inner.dropped += 1;
        }
        inner.items.push_back(item);
    }

    /// Never invokes the caller's iterator while holding the ring lock.
    pub fn extend(&self, items: impl IntoIterator<Item = T>) {
        for item in items {
            self.append(item);
        }
    }

    pub fn len(&self) -> usize {
        self.inner.lock().items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Consistent snapshot of all entries (oldest first).
    pub fn snapshot(&self) -> Vec<T> {
        self.inner.lock().items.iter().cloned().collect()
    }

    /// Python-style indexing: negative indexes count from the end.
    pub fn get(&self, index: isize) -> Option<T> {
        let inner = self.inner.lock();
        let len = inner.items.len() as isize;
        let idx = if index < 0 { len + index } else { index };
        if idx < 0 || idx >= len {
            return None;
        }
        inner.items.get(idx as usize).cloned()
    }

    /// Snapshot slice `[start, end)` with Python slice clamping semantics.
    pub fn slice(&self, start: Option<isize>, end: Option<isize>) -> Vec<T> {
        let inner = self.inner.lock();
        let len = inner.items.len() as isize;
        let norm = |v: isize| -> isize { if v < 0 { (len + v).max(0) } else { v.min(len) } };
        let s = norm(start.unwrap_or(0));
        let e = norm(end.unwrap_or(len));
        if s >= e {
            return Vec::new();
        }
        inner.items.range(s as usize..e as usize).cloned().collect()
    }

    pub fn clear(&self) {
        self.inner.lock().items.clear();
    }

    /// Atomically remove and return up to `limit` oldest entries.
    pub fn drain(&self, limit: Option<usize>) -> Vec<T> {
        let mut inner = self.inner.lock();
        let count = limit.map_or(inner.items.len(), |l| l.min(inner.items.len()));
        inner.items.drain(..count).collect()
    }
}

impl<T: Clone> Default for LogRing<T> {
    fn default() -> Self {
        Self::new(5000).expect("5000 > 0")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_drops_oldest_and_counts() {
        assert!(LogRing::<i32>::new(0).is_err());
        let r = LogRing::with_items(3, 1..=5).unwrap();
        assert_eq!(r.snapshot(), vec![3, 4, 5]);
        assert_eq!(r.dropped(), 2);
        assert_eq!(r.get(-1), Some(5));
        assert_eq!(r.slice(Some(-2), None), vec![4, 5]);
        assert_eq!(r.drain(Some(2)), vec![3, 4]);
        assert_eq!(r.len(), 1);
        r.clear();
        assert!(r.is_empty());
        assert_eq!(r.dropped_count(), 2);
    }
}

use std::collections::{BTreeMap, VecDeque};
use std::num::NonZeroUsize;
use std::time::Duration;

/// A limited-size vector where older elements are evicted first.
///
/// Elements are ordered by the provided timestamp upon insertion, followed by the order of insertion.
/// That means that element `u` is older than element `v` if and only if:
/// 1. The timestamp for the insertion of `u` is before the timestamp for the insertion of `v`.
/// 2. Or, if they both have the same timestamp for insertion, `u` was inserted before `v`.
///
/// # Examples
///
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimedSizedVec<T> {
    expiration: Duration,
    capacity: NonZeroUsize,
    size: usize,
    store: BTreeMap<Timestamp, VecDeque<T>>,
}

impl<T> TimedSizedVec<T> {
    /// Create a new empty [`TimedSizedVec`].
    ///
    /// # Examples
    ///
    /// Create a `TimeSizedVec` containing at most 10 elements which are no older than 1 minute.
    ///
    /// ```rust
    /// use std::num::NonZeroUsize;
    /// use std::time::Duration;
    /// use canhttp::multi::{TimedSizedVec, Timestamp};
    ///
    /// let mut vec = TimedSizedVec::new(Duration::from_secs(60), NonZeroUsize::new(10).unwrap());
    ///
    /// let _evicted = vec.insert_evict(Timestamp::from_nanos_since_unix_epoch(1), "a");
    /// ```
    pub fn new(expiration: Duration, capacity: NonZeroUsize) -> Self {
        Self {
            expiration,
            capacity,
            size: 0,
            store: BTreeMap::default(),
        }
    }

    /// TODO
    pub fn insert_evict(&mut self, now: Timestamp, value: T) -> BTreeMap<Timestamp, VecDeque<T>> {
        assert!(
            self.size <= self.capacity.get(),
            "BUG: expected at most {} elements, but got {}",
            self.capacity,
            self.size
        );
        let mut evicted = self.evict_expired(now);
        if self.size == self.capacity.get() {
            if let Some((timestamp, value)) = self.remove_oldest() {
                let values = evicted.entry(timestamp).or_default();
                values.push_front(value)
            }
        }
        assert!(
            self.size < self.capacity.get(),
            "BUG: expected at most {} elements, but got {}",
            self.capacity,
            self.size
        );
        let values = self.store.entry(now).or_default();
        values.push_back(value);
        self.size += 1;
        evicted
    }

    /// TODO
    pub fn evict_expired(&mut self, now: Timestamp) -> BTreeMap<Timestamp, VecDeque<T>> {
        match now.checked_sub(self.expiration) {
            Some(cutoff) => {
                let mut non_expired = self.store.split_off(&cutoff);
                std::mem::swap(&mut self.store, &mut non_expired);
                let expired = non_expired;
                // adjust size
                if expired.len() < self.store.len() {
                    let num_expired_elements = expired.values().map(|values| values.len()).sum();
                    self.size = self
                        .size
                        .checked_sub(num_expired_elements)
                        .expect("BUG: unexpected number of elements");
                } else {
                    self.size = self.store.values().map(|values| values.len()).sum()
                }
                expired
            }
            None => BTreeMap::default(),
        }
    }

    fn remove_oldest(&mut self) -> Option<(Timestamp, T)> {
        self.store.first_entry().and_then(|mut entry| {
            let timestamp = *entry.key();
            if let Some(removed) = entry.get_mut().pop_front() {
                self.size = self
                    .size
                    .checked_sub(1)
                    .expect("BUG: unexpected number of elements");
                if entry.get().is_empty() {
                    let _ = entry.remove();
                }
                return Some((timestamp, removed));
            }
            None
        })
    }

    /// TODO
    pub fn iter(&self) -> impl Iterator<Item = (&Timestamp, &T)> {
        self.store
            .iter()
            .flat_map(|(timestamp, values)| values.iter().map(move |value| (timestamp, value)))
    }

    /// Returns the number of elements.
    ///
    /// To avoid containing expired elements, use [`Self::unexpired_len`].
    pub fn len(&self) -> usize {
        self.size
    }

    /// Returns the number of non-expired elements by evicting expired elements first.
    pub fn unexpired_len(&mut self, now: Timestamp) -> usize {
        self.evict_expired(now);
        self.len()
    }

    /// Returns true if the vector contains no elements.
    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    /// Returns the maximum number of elements that can be stored.
    pub fn capacity(&self) -> NonZeroUsize {
        self.capacity
    }
}

/// Time in nanoseconds since the epoch (1970-01-01).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd)]
pub struct Timestamp(u64);

impl Timestamp {
    /// TODO
    pub const fn from_nanos_since_unix_epoch(nanos: u64) -> Self {
        Timestamp(nanos)
    }

    /// Checked `Time` subtraction with a `Duration`. Computes `self - rhs`,
    /// returning [`None`] if underflow occurs.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// use std::time::Duration;
    /// use canhttp::multi::Timestamp;
    ///
    /// assert_eq!(Timestamp::from_nanos_since_unix_epoch(3).checked_sub(Duration::from_nanos(2)), Some(Timestamp::from_nanos_since_unix_epoch(1)));
    /// assert_eq!(Timestamp::from_nanos_since_unix_epoch(2).checked_sub(Duration::from_nanos(3)), None);
    /// ```
    pub fn checked_sub(self, rhs: Duration) -> Option<Timestamp> {
        if let Ok(rhs_nanos) = u64::try_from(rhs.as_nanos()) {
            Some(Timestamp(self.0.checked_sub(rhs_nanos)?))
        } else {
            None
        }
    }
}

/// TODO
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimedSizedMap<K, V> {
    expiration: Duration,
    capacity: NonZeroUsize,
    store: BTreeMap<K, TimedSizedVec<V>>,
}

impl<K, V> TimedSizedMap<K, V> {
    /// TODO
    pub fn new(expiration: Duration, capacity: NonZeroUsize) -> Self {
        Self {
            expiration,
            capacity,
            store: BTreeMap::default(),
        }
    }

    /// TODO
    pub fn insert_evict(
        &mut self,
        now: Timestamp,
        key: K,
        value: V,
    ) -> BTreeMap<Timestamp, VecDeque<V>>
    where
        K: Ord,
    {
        let values = self
            .store
            .entry(key)
            .or_insert_with(|| TimedSizedVec::new(self.expiration, self.capacity));
        values.insert_evict(now, value)
    }

    /// TODO
    pub fn sort_keys_by<'a, ExtractSortKeyFn, SortKey>(
        &mut self,
        keys: &'a [&K],
        extractor: ExtractSortKeyFn,
    ) -> impl Iterator<Item = &'a K>
    where
        K: Ord,
        ExtractSortKeyFn: Fn(Option<&mut TimedSizedVec<V>>) -> SortKey,
        SortKey: Ord,
    {
        let mut sorted_keys = Vec::with_capacity(keys.len());
        for key in keys {
            let sort_key = extractor(self.store.get_mut(key));
            sorted_keys.push((sort_key, key));
        }
        sorted_keys.sort_by(|(left_sort_key, _left_key), (right_sort_key, _right_key)| {
            left_sort_key.cmp(right_sort_key)
        });
        sorted_keys.into_iter().map(|(_sort_key, key)| *key)
    }

    /// TODO
    pub fn iter(&self) -> impl Iterator<Item = (&K, &Timestamp, &V)> {
        self.store.iter().flat_map(|(k, values)| {
            values
                .iter()
                .map(move |(timestamp, value)| (k, timestamp, value))
        })
    }
}

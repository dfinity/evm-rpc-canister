use crate::multi::MultiResults;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

pub trait Reduce<K, V, E> {
    fn reduce(&self, results: MultiResults<K, V, E>) -> ReducedResult<K, V, E>;
}

pub type ReducedResult<K, V, E> = Result<V, ReductionError<K, V, E>>;

#[derive(Debug, PartialEq, Eq)]
pub enum ReductionError<K, V, E> {
    ConsistentError(E),
    InconsistentResults(MultiResults<K, V, E>),
}

impl<K, V, E> MultiResults<K, V, E> {
    pub fn reduce<R: Reduce<K, V, E>>(self, reducer: R) -> ReducedResult<K, V, E> {
        reducer.reduce(self)
    }
}

impl<K, V, E> MultiResults<K, V, E>
where
    E: PartialEq,
{
    fn expect_error(self) -> ReductionError<K, V, E> {
        if all_equal(&self.errors) && self.ok_results.is_empty() {
            return ReductionError::ConsistentError(self.errors.into_values().next().unwrap());
        }
        ReductionError::InconsistentResults(self)
    }
}

impl<K, V, E, T: Reduce<K, V, E>> Reduce<K, V, E> for Box<T> {
    fn reduce(&self, results: MultiResults<K, V, E>) -> ReducedResult<K, V, E> {
        self.as_ref().reduce(results)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ReduceWithEquality;

impl<K, V, E> Reduce<K, V, E> for ReduceWithEquality
where
    V: PartialEq,
    E: PartialEq,
{
    fn reduce(&self, results: MultiResults<K, V, E>) -> ReducedResult<K, V, E> {
        results.reduce_with_equality()
    }
}

impl<K, V, E> MultiResults<K, V, E>
where
    V: PartialEq,
    E: PartialEq,
{
    fn reduce_with_equality(self) -> ReducedResult<K, V, E> {
        assert!(
            !self.is_empty(),
            "ERROR: MultiResults is empty and cannot be reduced"
        );
        if !self.errors.is_empty() {
            return Err(self.expect_error());
        }
        if !all_equal(&self.ok_results) {
            return Err(ReductionError::InconsistentResults(self));
        }
        Ok(self.ok_results.into_values().next().unwrap())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReduceWithThreshold(u8);

impl ReduceWithThreshold {
    pub fn new(threshold: u8) -> Self {
        Self(threshold)
    }
}

impl<K, V, E> Reduce<K, V, E> for ReduceWithThreshold
where
    K: Ord + Clone,
    V: ToBytes,
    E: PartialEq,
{
    fn reduce(&self, results: MultiResults<K, V, E>) -> ReducedResult<K, V, E> {
        results.reduce_with_threshold(self.0)
    }
}

impl<K, V, E> MultiResults<K, V, E>
where
    K: Ord + Clone,
    V: ToBytes,
    E: PartialEq,
{
    fn reduce_with_threshold(mut self, min: u8) -> ReducedResult<K, V, E> {
        assert!(
            !self.is_empty(),
            "ERROR: MultiResults is empty and cannot be reduced"
        );
        assert!(min > 0, "BUG: min must be greater than 0");
        if self.ok_results.len() < min as usize {
            // At least total >= min were queried,
            // so there is at least one error
            return Err(self.expect_error());
        }
        let mut distribution = BTreeMap::new();
        for (key, value) in &self.ok_results {
            let hash = value.hash();
            distribution
                .entry(hash)
                .or_insert_with(BTreeSet::new)
                .insert(key);
        }
        let (_most_frequent_value, mut keys) = distribution
            .into_iter()
            .max_by_key(|(_value, keys)| keys.len())
            .expect("BUG: distribution should be non-empty");
        if keys.len() < min as usize {
            return Err(ReductionError::InconsistentResults(self));
        }
        let key_with_most_frequent_value = keys
            .pop_first()
            .expect("BUG: keys should contain at least min > 0 elements")
            .clone();
        Ok(self
            .ok_results
            .remove(&key_with_most_frequent_value)
            .expect("BUG: missing element"))
    }
}

/// Convert to bytes.
///
/// It's expected that different values will lead to a different result.
pub trait ToBytes {
    fn to_bytes(&self) -> Vec<u8>;

    fn hash(&self) -> [u8; 32] {
        use sha2::{Digest, Sha256};

        let mut hasher = Sha256::new();
        hasher.update(self.to_bytes());
        hasher.finalize().into()
    }
}

impl<T: Serialize> ToBytes for T {
    fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        ciborium::ser::into_writer(self, &mut buf).expect("failed to serialize type");
        buf
    }
}

fn all_equal<K, T: PartialEq>(map: &BTreeMap<K, T>) -> bool {
    let mut iter = map.values();
    let base_value = iter.next().expect("BUG: map should be non-empty");
    iter.all(|value| value == base_value)
}

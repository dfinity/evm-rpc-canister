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

/// Reduce a [`MultiResults`] by requiring that all elements are ok and all equal to each other.
/// 
/// # Examples
///
/// ```
/// use canhttp::multi::{MultiResults, ReduceWithEquality, ReduceWithThreshold, ReductionError};
///
/// let results: MultiResults<_, _, ()> = MultiResults::from_non_empty_iter(vec![
///     (0_u8, Ok("same")),
///     (1_u8, Ok("same")),
///     (2_u8, Ok("same"))
/// ]);
/// assert_eq!(
///     results.clone().reduce(ReduceWithEquality),
///     Ok("same")
/// );
/// 
/// let results = MultiResults::from_non_empty_iter(vec![
///     (0_u8, Ok("same")),
///     (1_u8, Err("unknown")),
///     (2_u8, Ok("same"))
/// ]);
/// assert_eq!(
///     results.clone().reduce(ReduceWithEquality),
///     Err(ReductionError::InconsistentResults(results))
/// )
/// ```
///
/// # Panics
///
/// If the results is empty.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ReduceWithEquality;

impl<K, V, E> Reduce<K, V, E> for ReduceWithEquality
where
    V: PartialEq,
    E: PartialEq,
{
    fn reduce(&self, results: MultiResults<K, V, E>) -> ReducedResult<K, V, E> {
        assert!(
            !results.is_empty(),
            "ERROR: MultiResults is empty and cannot be reduced"
        );
        if !results.errors.is_empty() {
            return Err(results.expect_error());
        }
        if !all_equal(&results.ok_results) {
            return Err(ReductionError::InconsistentResults(results));
        }
        Ok(results.ok_results.into_values().next().unwrap())
    }
}

/// Reduce a [`MultiResults`] by requiring that at least threshold many `Ok` results are the same.
/// 
/// # Examples
/// 
/// ```
/// use canhttp::multi::{MultiResults, ReduceWithThreshold, ReductionError};
/// let results = MultiResults::from_non_empty_iter(vec![
///     (0_u8, Ok("same")),
///     (1_u8, Err("unknown")),
///     (2_u8, Ok("same"))
/// ]);
/// assert_eq!(results.reduce(ReduceWithThreshold::new(2)), Ok("same"));
///
/// let results = MultiResults::from_non_empty_iter(vec![
///     (0_u8, Ok("same")),
///     (1_u8, Err("unknown")),
///     (2_u8, Ok("different"))
/// ]);
/// assert_eq!(
///     results.clone().reduce(ReduceWithThreshold::new(2)),
///     Err(ReductionError::InconsistentResults(results))
/// )
/// ```
/// 
/// # Panics
/// 
/// If the results is empty.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReduceWithThreshold(u8);

impl ReduceWithThreshold {
    /// Instantiate [`ReduceWithThreshold`] with the given threshold.
    ///
    /// # Panics
    ///
    /// If the threshold is 0.
    pub fn new(threshold: u8) -> Self {
        assert!(threshold > 0, "ERROR: min must be greater than 0");
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
        assert!(
            !results.is_empty(),
            "ERROR: MultiResults is empty and cannot be reduced"
        );
        let min = self.0;
        if results.ok_results.len() < min as usize {
            if !results.errors.is_empty() {
                return Err(results.expect_error());
            }
            return Err(ReductionError::InconsistentResults(results));
        }
        let mut distribution = BTreeMap::new();
        for (key, value) in &results.ok_results {
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
            return Err(ReductionError::InconsistentResults(results));
        }
        let key_with_most_frequent_value = keys
            .pop_first()
            .expect("BUG: keys should contain at least min > 0 elements")
            .clone();
        let mut results = results;
        Ok(results
            .ok_results
            .remove(&key_with_most_frequent_value)
            .expect("BUG: missing element"))
    }
}

/// Convert to bytes.
///
/// It's expected that different values will lead to a different result.
pub trait ToBytes {
    /// Convert to bytes.
    fn to_bytes(&self) -> Vec<u8>;

    /// Hash the converted bytes on 32 bytes.
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

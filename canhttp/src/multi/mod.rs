#[cfg(test)]
mod tests;

use futures_channel::mpsc;
use futures_util::StreamExt;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Debug;
use tower::{Service, ServiceExt};

pub async fn parallel_call<S, I, RequestId, Request, Response, Error>(
    service: S,
    requests: I,
) -> (S, MultiResults<RequestId, Response, Error>)
where
    S: Service<Request, Response = Response, Error = Error>,
    I: IntoIterator<Item = (RequestId, Request)>,
    RequestId: Ord,
{
    let (tx_id, rx_id) = mpsc::unbounded();
    let (tx, rx) = mpsc::unbounded();
    let responses = service.call_all(rx);
    for (id, request) in requests.into_iter() {
        tx_id.unbounded_send(id).expect("BUG: channel closed");
        tx.unbounded_send(request).expect("BUG: channel closed");
    }
    drop(tx_id);
    drop(tx);
    let mut results = MultiResults::default();
    let mut zip = rx_id.zip(responses);
    // Responses arrive in the same order as the requests
    // call_all uses under the hood FuturesOrdered
    while let Some((id, response)) = zip.next().await {
        results.insert_once(id, response);
    }
    let (_, parallel_service) = zip.into_inner();
    (parallel_service.into_inner(), results)
}

#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord)]
pub struct RequestIndex(u64);

/// Aggregates responses from multiple requests.
///
/// Typically, those requests are the same excepted for the URL.
/// This is useful to verify that the responses are consistent between each other
/// and avoid a single point of failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultiResults<K, V, E> {
    ok_results: BTreeMap<K, V>,
    errors: BTreeMap<K, E>,
}

impl<K, V, E> Default for MultiResults<K, V, E> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K, V, E> MultiResults<K, V, E> {
    pub fn new() -> Self {
        Self {
            ok_results: BTreeMap::new(),
            errors: BTreeMap::new(),
        }
    }

    pub fn into_inner(self) -> (BTreeMap<K, V>, BTreeMap<K, E>) {
        (self.ok_results, self.errors)
    }

    pub fn len(&self) -> usize {
        self.ok_results.len() + self.errors.len()
    }

    fn is_empty(&self) -> bool {
        self.ok_results.is_empty() && self.errors.is_empty()
    }
}

impl<K: Ord, V, E> MultiResults<K, V, E> {
    pub fn from_non_empty_iter<I>(iter: I) -> Self
    where
        I: IntoIterator<Item = (K, Result<V, E>)>,
    {
        let mut results = MultiResults::default();
        for (key, result) in iter {
            results.insert_once(key, result);
        }
        assert!(!results.is_empty(), "ERROR: MultiResults cannot be empty");
        results
    }

    pub fn insert_once(&mut self, key: K, result: Result<V, E>) {
        match result {
            Ok(value) => {
                self.insert_once_ok(key, value);
            }
            Err(error) => {
                self.insert_once_err(key, error);
            }
        }
    }

    pub fn insert_once_ok(&mut self, key: K, value: V) {
        assert!(!self.errors.contains_key(&key));
        assert!(self.ok_results.insert(key, value).is_none());
    }

    pub fn insert_once_err(&mut self, key: K, error: E) {
        assert!(!self.ok_results.contains_key(&key));
        assert!(self.errors.insert(key, error).is_none());
    }

    pub fn add_errors<I>(&mut self, errors: I)
    where
        I: IntoIterator<Item = (K, E)>,
    {
        for (key, error) in errors.into_iter() {
            self.insert_once_err(key, error);
        }
    }
}

pub type ReducedResult<K, V, E> = Result<V, ReductionError<K, V, E>>;

#[derive(Debug, PartialEq, Eq)]
pub enum ReductionError<K, V, E> {
    ConsistentError(E),
    InconsistentResults(MultiResults<K, V, E>),
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

impl<K, V, E> MultiResults<K, V, E>
where
    V: PartialEq,
    E: PartialEq,
{
    pub fn reduce_with_equality(self) -> ReducedResult<K, V, E> {
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

impl<K, V, E> MultiResults<K, V, E>
where
    K: Ord + Clone,
    V: ToBytes,
    E: PartialEq,
{
    pub fn reduce_with_threshold(mut self, min: u8) -> ReducedResult<K, V, E> {
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
            let wrapped_value = OrdByHash::new(value);
            distribution
                .entry(wrapped_value)
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

fn all_equal<K, T: PartialEq>(map: &BTreeMap<K, T>) -> bool {
    let mut iter = map.values();
    let base_value = iter.next().expect("BUG: map should be non-empty");
    iter.all(|value| value == base_value)
}

/// Convert to bytes.
/// 
/// It's expected that different values will lead to a different result.
pub trait ToBytes {
    fn to_bytes(&self) -> Vec<u8>;
}

struct OrdByHash<'a, T> {
    hash: [u8; 32],
    value: &'a T,
}

impl<'a, T: ToBytes> OrdByHash<'a, T> {
    pub fn new(value: &'a T) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(&value.to_bytes());
        let hash = hasher.finalize().into();
        Self { hash, value }
    }
}

impl<'a, T> PartialEq for OrdByHash<'a, T> {
    fn eq(&self, other: &Self) -> bool {
        self.hash == other.hash
    }
}

impl<'a, T> PartialOrd for OrdByHash<'a, T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.hash.partial_cmp(&other.hash)
    }
}

impl<'a, T> Eq for OrdByHash<'a, T> {}

impl<'a, T> Ord for OrdByHash<'a, T> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.hash.cmp(&other.hash)
    }
}

impl<T: Serialize> ToBytes for T {
    fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        ciborium::ser::into_writer(self, &mut buf).expect("failed to serialize type");
        buf
    }
}

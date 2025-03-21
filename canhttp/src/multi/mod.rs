pub use reduce::{
    Reduce, ReduceWithEquality, ReduceWithThreshold, ReducedResult, ReductionError, ToBytes,
};

mod reduce;
#[cfg(test)]
mod tests;

use futures_channel::mpsc;
use futures_util::StreamExt;
use std::collections::BTreeMap;
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

    pub fn is_empty(&self) -> bool {
        self.ok_results.is_empty() && self.errors.is_empty()
    }

    pub fn into_vec(self) -> Vec<(K, Result<V, E>)> {
        self.ok_results
            .into_iter()
            .map(|(k, result)| (k, Ok(result)))
            .chain(self.errors.into_iter().map(|(k, error)| (k, Err(error))))
            .collect()
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

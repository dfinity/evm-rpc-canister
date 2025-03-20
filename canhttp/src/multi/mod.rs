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
}

impl<K: Ord, V, E> MultiResults<K, V, E> {
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
    V: PartialEq,
    E: PartialEq,
{
    fn reduce_with_equality(self) -> ReducedResult<K, V, E> {
        if !self.errors.is_empty() {
            if all_equal(&self.errors) && self.ok_results.is_empty() {
                return Err(ReductionError::ConsistentError(
                    self.errors.into_values().next().unwrap(),
                ));
            }
            return Err(ReductionError::InconsistentResults(self));
        }
        if !all_equal(&self.ok_results) {
            return Err(ReductionError::InconsistentResults(self));
        }
        Ok(self.ok_results.into_values().next().unwrap())
    }
}

fn all_equal<K, T: PartialEq>(map: &BTreeMap<K, T>) -> bool {
    let mut iter = map.values();
    let base_value = iter.next().expect("BUG: map should be non-empty");
    iter.all(|value| value == base_value)
}

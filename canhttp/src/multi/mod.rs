//! Make multiple calls in parallel to a [`tower::Service`] and handle their multiple results.
//! See [`parallel_call`].

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

/// Process all requests from the given iterator and produce a result for reach request.
///
/// The iterator yields a pair containing:
/// 1. An ID *uniquely* identifying this request.
/// 2. The request itself
///
/// The requests will be sent to the underlying service in parallel and the result for each request
/// can be retrieved by the corresponding request ID.
/// 
/// # Examples
///
/// ```rust
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// use std::convert::Infallible;
/// use tower::ServiceBuilder;
/// use canhttp::multi::parallel_call;
///
/// let adding_service =
///     ServiceBuilder::new().service_fn(|(left, right): (u32, u32)| async move {
///         Ok::<_, Infallible>(left + right)
///     });
///
/// let (_service, results) =
///     parallel_call(adding_service, vec![(0, (2, 3)), (1, (4, 5))]).await;
///
/// assert_eq!(results.get(&0).unwrap(), Ok(&5_u32));
/// assert_eq!(results.get(&1).unwrap(), Ok(&9_u32));
/// # Ok(())
/// # }
/// ```
///
/// # Panics
///
/// If two requests produced by the iterator have the same request ID.
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

    pub fn get(&self, id: &K) -> Option<Result<&V, &E>> {
        self.ok_results
            .get(id)
            .map(Ok)
            .or_else(|| self.errors.get(id).map(Err))
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
        assert!(
            !self.errors.contains_key(&key),
            "ERROR: duplicate key in `errors`"
        );
        assert!(
            self.ok_results.insert(key, value).is_none(),
            "ERROR: duplicate key in `ok_results`"
        );
    }

    pub fn insert_once_err(&mut self, key: K, error: E) {
        assert!(
            !self.ok_results.contains_key(&key),
            "ERROR: duplicate key in `ok_results`"
        );
        assert!(
            self.errors.insert(key, error).is_none(),
            "ERROR: duplicate key in `errors`"
        );
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

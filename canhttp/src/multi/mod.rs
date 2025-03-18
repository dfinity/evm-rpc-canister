use futures_channel::mpsc;
use futures_util::StreamExt;
use std::collections::BTreeMap;
use tower::{Service, ServiceExt};

pub async fn parallel_call<S, Request, Response, Error>(
    service: S,
    requests: Vec<Request>,
) -> (S, Vec<Result<Response, Error>>)
where
    S: Service<Request, Response = Response, Error = Error>,
{
    let (tx, rx) = mpsc::unbounded();
    let mut responses = service.call_all(rx);
    let mut result = Vec::with_capacity(requests.len());
    for request in requests {
        tx.unbounded_send(request).expect("BUG: channel closed")
    }
    drop(tx);
    while let Some(response) = responses.next().await {
        result.push(response)
    }
    (responses.into_inner(), result)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestIndex(u64);

/// Aggregates responses of different providers to the same query.
/// Guaranteed to be non-empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultiCallResults<Response, Error> {
    ok_results: BTreeMap<RequestIndex, Response>,
    errors: BTreeMap<RequestIndex, Error>,
}

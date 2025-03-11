//! This module contains definitions for communicating witEthereum API using the [JSON RPC](https://ethereum.org/en/developers/docs/apis/json-rpc/)
//! interface.

use crate::http::http_client;
use crate::memory::get_override_provider;
use crate::providers::resolve_rpc_service;
use crate::rpc_client::eth_rpc_error::{sanitize_send_raw_transaction_result, Parser};
use crate::rpc_client::json::responses::{Block, FeeHistory, LogEntry, TransactionReceipt};
use crate::rpc_client::numeric::{TransactionCount, Wei};
use crate::types::MetricRpcMethod;
use candid::candid_method;
use canhttp::{
    http::json::{JsonRpcRequestBody, JsonRpcResponseBody},
    MaxResponseBytesRequestExtension, TransformContextRequestExtension,
};
use evm_rpc_types::{JsonRpcError, RpcError, RpcService};
use ic_cdk::api::management_canister::http_request::{
    HttpResponse, TransformArgs, TransformContext,
};
use ic_cdk_macros::query;
use minicbor::{Decode, Encode};
use serde::{de::DeserializeOwned, Serialize};
use std::fmt;
use std::fmt::Debug;

#[cfg(test)]
mod tests;

// This constant is our approximation of the expected header size.
// The HTTP standard doesn't define any limit, and many implementations limit
// the headers size to 8 KiB. We chose a lower limit because headers observed on most providers
// fit in the constant defined below, and if there is a spike, then the payload size adjustment
// should take care of that.
pub const HEADER_SIZE_LIMIT: u64 = 2 * 1024;

// This constant comes from the IC specification:
// > If provided, the value must not exceed 2MB
const HTTP_MAX_SIZE: u64 = 2_000_000;

pub const MAX_PAYLOAD_SIZE: u64 = HTTP_MAX_SIZE - HEADER_SIZE_LIMIT;

/// Describes a payload transformation to execute before passing the HTTP response to consensus.
/// The purpose of these transformations is to ensure that the response encoding is deterministic
/// (the field order is the same).
#[derive(Debug, Decode, Encode)]
pub enum ResponseTransform {
    #[n(0)]
    Block,
    #[n(1)]
    LogEntries,
    #[n(2)]
    TransactionReceipt,
    #[n(3)]
    FeeHistory,
    #[n(4)]
    SendRawTransaction,
}

impl ResponseTransform {
    fn apply(&self, body_bytes: &mut Vec<u8>) {
        fn redact_response<T>(body: &mut Vec<u8>)
        where
            T: Serialize + DeserializeOwned,
        {
            let response: JsonRpcResponseBody<T> = match serde_json::from_slice(body) {
                Ok(response) => response,
                Err(_) => return,
            };
            *body = serde_json::to_string(&response)
                .expect("BUG: failed to serialize response")
                .into_bytes();
        }

        fn redact_collection_response<T>(body: &mut Vec<u8>)
        where
            T: Serialize + DeserializeOwned,
        {
            let mut response: JsonRpcResponseBody<Vec<T>> = match serde_json::from_slice(body) {
                Ok(response) => response,
                Err(_) => return,
            };

            if let Ok(result) = response.as_result_mut() {
                sort_by_hash(result);
            }

            *body = serde_json::to_string(&response)
                .expect("BUG: failed to serialize response")
                .into_bytes();
        }

        match self {
            Self::Block => redact_response::<Block>(body_bytes),
            Self::LogEntries => redact_collection_response::<LogEntry>(body_bytes),
            Self::TransactionReceipt => redact_response::<TransactionReceipt>(body_bytes),
            Self::FeeHistory => redact_response::<FeeHistory>(body_bytes),
            Self::SendRawTransaction => {
                sanitize_send_raw_transaction_result(body_bytes, Parser::new())
            }
        }
    }
}

#[query]
#[candid_method(query)]
fn cleanup_response(mut args: TransformArgs) -> HttpResponse {
    args.response.headers.clear();
    let status_ok = args.response.status >= 200u16 && args.response.status < 300u16;
    if status_ok && !args.context.is_empty() {
        let maybe_transform: Result<ResponseTransform, _> = minicbor::decode(&args.context[..]);
        if let Ok(transform) = maybe_transform {
            transform.apply(&mut args.response.body);
        }
    }
    args.response
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ResponseSizeEstimate(u64);

impl ResponseSizeEstimate {
    pub fn new(num_bytes: u64) -> Self {
        assert!(num_bytes > 0);
        assert!(num_bytes <= MAX_PAYLOAD_SIZE);
        Self(num_bytes)
    }

    /// Describes the expected (90th percentile) number of bytes in the HTTP response body.
    /// This number should be less than `MAX_PAYLOAD_SIZE`.
    pub fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Display for ResponseSizeEstimate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

pub trait HttpResponsePayload {
    fn response_transform() -> Option<ResponseTransform> {
        None
    }
}

impl<T: HttpResponsePayload> HttpResponsePayload for Option<T> {}

impl HttpResponsePayload for TransactionCount {}

impl HttpResponsePayload for Wei {}

/// Calls a JSON-RPC method on an Ethereum node at the specified URL.
pub async fn call<I, O>(
    provider: &RpcService,
    method: impl Into<String>,
    params: I,
    response_size_estimate: ResponseSizeEstimate,
) -> Result<O, RpcError>
where
    I: Serialize + Clone + Debug,
    O: Debug + DeserializeOwned + HttpResponsePayload,
{
    use tower::Service;

    let transform_op = O::response_transform()
        .as_ref()
        .map(|t| {
            let mut buf = vec![];
            minicbor::encode(t, &mut buf).unwrap();
            buf
        })
        .unwrap_or_default();

    let effective_size_estimate = response_size_estimate.get();
    let request = resolve_rpc_service(provider.clone())?
        .post(&get_override_provider())?
        .max_response_bytes(effective_size_estimate)
        .transform_context(TransformContext::from_name(
            "cleanup_response".to_owned(),
            transform_op.clone(),
        ))
        .body(JsonRpcRequestBody::new(method, params))
        .expect("BUG: invalid request");

    let eth_method = request.body().method().to_string();
    let mut client = http_client(MetricRpcMethod(eth_method.clone()), true);
    let response = client.call(request).await?;
    match response.into_body().into_result() {
        Ok(r) => Ok(r),
        Err(canhttp::http::json::JsonRpcError {
            code,
            message,
            data: _,
        }) => Err(JsonRpcError { code, message }.into()),
    }
}

fn sort_by_hash<T: Serialize + DeserializeOwned>(to_sort: &mut [T]) {
    use ic_sha3::Keccak256;
    to_sort.sort_by(|a, b| {
        let a_hash = Keccak256::hash(serde_json::to_vec(a).expect("BUG: failed to serialize"));
        let b_hash = Keccak256::hash(serde_json::to_vec(b).expect("BUG: failed to serialize"));
        a_hash.cmp(&b_hash)
    });
}

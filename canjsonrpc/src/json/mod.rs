use http::StatusCode;
use ic_cdk::api::management_canister::http_request::{
    CanisterHttpRequestArgument, HttpHeader, HttpMethod, HttpResponse, TransformContext,
};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use thiserror::Error;
use tower::{BoxError, Service};

/// An envelope for all JSON-RPC requests.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JsonRpcRequest<T> {
    pub jsonrpc: String,
    pub method: String,
    pub id: u64,
    pub params: T,
}

impl<T> JsonRpcRequest<T> {
    pub fn new(id: u64, method: String, params: T) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            method,
            id,
            params,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JsonRpcResponse<T> {
    pub id: u64,
    pub jsonrpc: String,
    #[serde(flatten)]
    pub result: JsonRpcResult<T>,
}

/// An envelope for all JSON-RPC replies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum JsonRpcResult<T> {
    #[serde(rename = "result")]
    Result(T),
    #[serde(rename = "error")]
    Error { code: i64, message: String },
}

#[derive(Debug)]
pub struct CanisterJsonRpcRequestArgument {
    url: String,
    max_response_bytes: Option<u64>,
    headers: Vec<HttpHeader>,
    json_rpc_request: JsonRpcRequest<serde_json::Value>,
    transform: Option<TransformContext>,
}

impl TryFrom<CanisterJsonRpcRequestArgument> for CanisterHttpRequestArgument {
    type Error = JsonRpcError;

    fn try_from(request: CanisterJsonRpcRequestArgument) -> Result<Self, Self::Error> {
        let body = serde_json::to_vec(&request.json_rpc_request)
            .map_err(|e| JsonRpcError::JsonSerializationError(e.to_string()))?;
        Ok(CanisterHttpRequestArgument {
            url: request.url,
            max_response_bytes: request.max_response_bytes,
            method: HttpMethod::POST,
            headers: request.headers,
            body: Some(body),
            transform: request.transform,
        })
    }
}

type JsonRpcRequestGeneric = JsonRpcRequest<serde_json::Value>;
type JsonRpcResponseGeneric = JsonRpcResponse<serde_json::Value>;

pub struct JsonRpcClient<S> {
    service: S,
    initial_response_size_estimate: u64,
}

struct JsonRpcResponseFuture<F> {
    response_future: F,
}

impl<F, Error> Future for JsonRpcResponseFuture<F>
where
    F: Future<Output = Result<HttpResponse, Error>>,
    Error: Into<BoxError>,
{
    type Output = Result<JsonRpcResponseGeneric, JsonRpcError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        todo!()
    }
}

pub fn is_successful_http_code(status: &u16) -> bool {
    const OK: u16 = 200;
    const REDIRECTION: u16 = 300;
    (OK..REDIRECTION).contains(status)
}

pub fn http_status_code(response: &HttpResponse) -> u16 {
    use num_traits::cast::ToPrimitive;
    // HTTP status code are always 3 decimal digits, hence at most 999.
    // See https://httpwg.org/specs/rfc9110.html#status.code.extensibility
    response.status.0.to_u16().expect("valid HTTP status code")
}

#[derive(Error, Debug)]
pub enum JsonRpcError {
    #[error("Failed to serialized request: {0}")]
    JsonSerializationError(String),
    /// Response is not a valid JSON-RPC response,
    /// which means that the response was not successful (status other than 2xx)
    /// or that the response body could not be deserialized into a JSON-RPC response.
    #[error("Invalid HTTP JSON-RPC response: status {status}, body: {body}, parsing error: {parsing_error:?}"
    )]
    InvalidHttpJsonRpcResponse {
        status: u16,
        body: String,
        parsing_error: Option<String>,
    },
}

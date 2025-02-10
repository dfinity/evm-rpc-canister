use crate::http::{AddMaxResponseBytesExtension, AddTransformContextExtension};
use bytes::Bytes;
use http::{Request, StatusCode};
use ic_cdk::api::management_canister::http_request::{
    CanisterHttpRequestArgument, HttpHeader, HttpMethod, HttpResponse, TransformContext,
};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use thiserror::Error;
use tower::{BoxError, Layer, Service};

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

// impl CanisterJsonRpcRequestArgument {
//     pub fn builder<I>(request: JsonRpcRequest<I>) -> CanisterJsonRpcRequestArgumentBuilder<I> {}
// }
//
// pub struct CanisterJsonRpcRequestArgumentBuilder<I> {
//     json_rpc_request: JsonRpcRequest<I>,
// }
//
// impl<I> CanisterJsonRpcRequestArgumentBuilder<I>
// where
//     I: Serialize,
// {
//     pub fn build(self, url: String) -> Result<CanisterJsonRpcRequestArgument, JsonRpcError> {}
// }

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

pub struct JsonRpcRequestBuilder {
    inner: http::request::Builder,
    method: String,
    id: u64,
}

impl JsonRpcRequestBuilder {
    pub fn new(method: String) -> Self {
        let inner = http::Request::builder()
            .method("POST")
            .header("Content-Type", "application/json");
        let id = 0;
        Self { inner, method, id }
    }

    pub fn build_with_params<T>(self, params: T) -> http::Result<http::Request<JsonRpcRequest<T>>> {
        self.inner.body(JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            method: self.method,
            id: self.id,
            params,
        })
    }
}

impl AddMaxResponseBytesExtension for JsonRpcRequestBuilder {
    fn max_response_bytes(mut self, value: u64) -> Self {
        self.inner = self.inner.max_response_bytes(value);
        self
    }
}

impl AddTransformContextExtension for JsonRpcRequestBuilder {
    fn transform_context(mut self, transform: TransformContext) -> Self {
        self.inner = self.inner.transform_context(transform);
        self
    }
}

#[derive(Debug, Clone, Copy)]
pub struct JsonRpcLayer;

#[derive(Debug, Clone, Copy)]
pub struct JsonRpcService<S> {
    service: S,
}

impl<S> Layer<S> for JsonRpcLayer {
    type Service = JsonRpcService<S>;

    fn layer(&self, service: S) -> Self::Service {
        Self::Service { service }
    }
}

impl<S> Service<http::Request<JsonRpcRequest<serde_json::Value>>> for JsonRpcService<S>
where
    S: Service<http::Request<Bytes>, Response = http::Response<Bytes>>,
    S::Error: Into<BoxError>,
{
    type Response = http::Response<JsonRpcResponse<serde_json::Value>>;
    type Error = BoxError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        todo!()
    }

    fn call(&mut self, req: Request<JsonRpcRequest<Value>>) -> Self::Future {
        todo!()
    }
}

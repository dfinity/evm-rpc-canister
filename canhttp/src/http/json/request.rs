use crate::convert::Convert;
use crate::http::HttpRequest;
use http::header::CONTENT_TYPE;
use http::HeaderValue;
use serde::{Deserialize, Serialize};
use std::marker::PhantomData;
use thiserror::Error;

/// Convert requests of type [`http::Request<T>`],
/// where `T` is `Serializable`, into [`HttpRequest`].
#[derive(Debug)]
pub struct JsonRequestConverter<T> {
    _marker: PhantomData<T>,
}

impl<T> JsonRequestConverter<T> {
    /// Create a new instance of [`JsonRequestConverter`].
    pub fn new() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

// #[derive(Clone)] would otherwise introduce a bound T: Clone, which is not needed.
impl<T> Clone for JsonRequestConverter<T> {
    fn clone(&self) -> Self {
        Self {
            _marker: self._marker,
        }
    }
}

impl<T> Default for JsonRequestConverter<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Error return when converting requests with [`JsonRequestConverter`].
#[derive(Error, Clone, Debug, Eq, PartialEq)]
pub enum JsonRequestConversionError {
    /// Request body failed to be serialized.
    #[error("Invalid JSON body: {0}")]
    InvalidJson(String),
}

impl<T> Convert<http::Request<T>> for JsonRequestConverter<T>
where
    T: Serialize,
{
    type Output = HttpRequest;
    type Error = JsonRequestConversionError;

    fn try_convert(&mut self, request: http::Request<T>) -> Result<Self::Output, Self::Error> {
        try_serialize_request(request)
            .map(add_content_type_header_if_missing)
            .map_err(Into::into)
    }
}

fn try_serialize_request<T>(
    request: http::Request<T>,
) -> Result<HttpRequest, JsonRequestConversionError>
where
    T: Serialize,
{
    let (parts, body) = request.into_parts();
    let json_body = serde_json::to_vec(&body)
        .map_err(|e| JsonRequestConversionError::InvalidJson(e.to_string()))?;
    Ok(HttpRequest::from_parts(parts, json_body))
}

fn add_content_type_header_if_missing(mut request: HttpRequest) -> HttpRequest {
    if !request.headers().contains_key(CONTENT_TYPE) {
        request
            .headers_mut()
            .insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    }
    request
}

/// JSON-RPC request.
pub type HttpJsonRpcRequest<T> = http::Request<JsonRpcRequestBody<T>>;

/// Body for all JSON-RPC requests, see the [specification](https://www.jsonrpc.org/specification).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JsonRpcRequestBody<T> {
    jsonrpc: String,
    method: String,
    id: Option<serde_json::Value>,
    params: Option<T>,
}

impl<T> JsonRpcRequestBody<T> {
    /// Create a new body of a JSON-RPC request.
    pub fn new(method: impl Into<String>, params: T) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            method: method.into(),
            id: Some(serde_json::Value::Number(0.into())),
            params: Some(params),
        }
    }

    /// Change the request ID.
    pub fn set_id(&mut self, id: u64) {
        self.id = Some(serde_json::Value::Number(id.into()));
    }

    /// Returns the request ID, if any.
    pub fn id(&self) -> Option<&serde_json::Value> {
        self.id.as_ref()
    }

    /// Returns the JSON-RPC method.
    pub fn method(&self) -> &str {
        &self.method
    }
}

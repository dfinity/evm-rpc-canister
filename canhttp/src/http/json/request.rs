use crate::http::HttpRequest;
use http::header::CONTENT_TYPE;
use http::HeaderValue;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tower::filter::Predicate;
use tower::BoxError;
use tower_layer::Layer;

pub struct JsonRequestFilter;

#[derive(Error, Clone, Debug, Eq, PartialEq)]
pub enum JsonRequestConversionError {
    #[error("Invalid JSON body: {0}")]
    InvalidJson(String),
}

impl<T> Predicate<http::Request<T>> for JsonRequestFilter
where
    T: Serialize,
{
    type Request = HttpRequest;

    fn check(&mut self, request: http::Request<T>) -> Result<Self::Request, BoxError> {
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

pub struct JsonRequestConversionLayer;

impl<S> Layer<S> for JsonRequestConversionLayer {
    type Service = tower::filter::Filter<S, JsonRequestFilter>;

    fn layer(&self, inner: S) -> Self::Service {
        tower::filter::Filter::new(inner, JsonRequestFilter)
    }
}

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
    pub fn new(method: impl Into<String>, params: T) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            method: method.into(),
            id: None,
            params: Some(params),
        }
    }

    pub fn set_id(&mut self, id: u64) {
        self.id = Some(serde_json::Value::Number(id.into()));
    }

    pub fn method(&self) -> &str {
        &self.method
    }
}

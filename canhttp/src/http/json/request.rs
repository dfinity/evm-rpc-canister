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

/// An envelope for all JSON-RPC requests.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JsonRpcRequest<T> {
    pub jsonrpc: String,
    pub method: String,
    pub id: u64,
    pub params: T,
}

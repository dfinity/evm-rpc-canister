//! Middleware to add a JSON translation layer (over HTTP).
//!
//! Transforms a low-level service that transmits bytes into one that transmits JSON payloads:
//!
//! ```text
//!                 │                     ▲              
//! http::Request<I>│                     │http::Response<O>
//!               ┌─┴─────────────────────┴───┐          
//!               │   JsonResponseConverter   │          
//!               └─┬─────────────────────▲───┘          
//!                 │                     │              
//!               ┌─▼─────────────────────┴───┐          
//!               │   JsonRequestConverter    │          
//!               └─┬─────────────────────┬───┘          
//!      HttpRequest│                     │HttpResponse
//!                 ▼                     │              
//!               ┌─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─┐
//!               │          SERVICE          │
//!               └─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─┘
//! ```
//! This can be used to transmit any kind of JSON payloads, such as JSON RPC over HTTP.
//!
//! # Examples
//!
//! ```rust
//! use canhttp::http::{HttpRequest, HttpResponse, json::JsonConversionLayer};
//! use ic_cdk::management_canister::{HttpRequestArgs as IcHttpRequest, HttpRequestResult as IcHttpResponse};
//! use tower::{Service, ServiceBuilder, ServiceExt, BoxError};
//! use serde_json::json;
//!
//! async fn echo_bytes(request: HttpRequest) -> Result<HttpResponse, BoxError> {
//!     Ok(http::Response::new(request.into_body()))
//! }
//!
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut service = ServiceBuilder::new()
//!   .layer(JsonConversionLayer::<serde_json::Value, serde_json::Value>::new())
//!   .service_fn(echo_bytes);
//!
//! let request = http::Request::post("https://internetcomputer.org")
//!   .header("Content-Type", "application/json")
//!   .body(json!({"key": "value"}))
//!   .unwrap();
//!
//! let response = service.ready().await.unwrap().call(request).await.unwrap();
//!
//! assert_eq!(response.into_body()["key"], "value");
//! # Ok(())
//! # }

use crate::convert::{ConvertRequest, ConvertRequestLayer, ConvertResponse, ConvertResponseLayer};
pub use id::{ConstantSizeId, Id};
pub use request::{
    HttpJsonRpcRequest, JsonRequestConversionError, JsonRequestConverter, JsonRpcRequest,
};
pub use response::{
    ConsistentJsonRpcIdFilter, ConsistentResponseIdFilterError, CreateJsonRpcIdFilter,
    HttpJsonRpcResponse, JsonResponseConversionError, JsonResponseConverter, JsonRpcError,
    JsonRpcResponse, JsonRpcResult,
};
pub use version::Version;

use serde::de::DeserializeOwned;
use serde::Serialize;
use std::marker::PhantomData;
use tower_layer::Layer;

#[cfg(test)]
mod tests;

mod id;
mod request;
mod response;
mod version;

/// Middleware that combines [`JsonRequestConverter`] to convert requests
/// and [`JsonResponseConverter`] to convert responses to a [`Service`].
///
/// See the [module docs](crate::http::json) for an example.
///
/// [`Service`]: tower::Service
#[derive(Debug)]
pub struct JsonConversionLayer<I, O> {
    _marker: PhantomData<(I, O)>,
}

impl<I, O> JsonConversionLayer<I, O> {
    /// Returns a new [`JsonConversionLayer`].
    pub fn new() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

impl<I, O> Clone for JsonConversionLayer<I, O> {
    fn clone(&self) -> Self {
        Self {
            _marker: self._marker,
        }
    }
}

impl<I, O> Default for JsonConversionLayer<I, O> {
    fn default() -> Self {
        Self::new()
    }
}

impl<S, I, O> Layer<S> for JsonConversionLayer<I, O>
where
    I: Serialize,
    O: DeserializeOwned,
{
    type Service =
        ConvertResponse<ConvertRequest<S, JsonRequestConverter<I>>, JsonResponseConverter<O>>;

    fn layer(&self, inner: S) -> Self::Service {
        let stack = tower_layer::Stack::new(
            ConvertRequestLayer::new(JsonRequestConverter::<I>::new()),
            ConvertResponseLayer::new(JsonResponseConverter::<O>::new()),
        );
        stack.layer(inner)
    }
}

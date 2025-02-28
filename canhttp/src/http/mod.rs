//! Middleware to add an HTTP translation layer.
//!
//! Transforms a low-level service that uses Candid types ([`IcHttpRequest`] and [`IcHttpResponse`])
//! into one that uses types from the [http](https://crates.io/crates/http) crate.
//!
//! ```text
//!              │                     ▲              
//! http::Request│                     │http::Response
//!            ┌─┴─────────────────────┴───┐          
//!            │HttpResponseConversionLayer│          
//!            └─┬─────────────────────▲───┘          
//!              │                     │              
//!            ┌─▼─────────────────────┴───┐          
//!            │HttpRequestConversionLayer │          
//!            └─┬─────────────────────┬───┘          
//! IcHttpRequest│                     │IcHttpResponse
//!              ▼                     │              
//!            ┌─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─┐
//!            │          SERVICE          │
//!            └─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─┘
//! ```
//!
//! This brings several advantages:
//!
//! * Can re-use existing types like [`http::request::Builder`] or [`http::StatusCode`].
//! * Requests are automatically sanitized and canonicalized (e.g. header names are validated and lower cased).
//! * Can re-use existing middlewares, like from the [tower-http](https://crates.io/crates/tower-http) crate.
//!
//! # Examples
//!
//! ```rust
//! use canhttp::{http::{HttpConversionLayer, MaxResponseBytesRequestExtension}, IcError};
//! use ic_cdk::api::management_canister::http_request::{CanisterHttpRequestArgument as IcHttpRequest, HttpResponse as IcHttpResponse};
//! use tower::{Service, ServiceBuilder, ServiceExt};
//!
//! async fn always_200_ok(request: IcHttpRequest) -> Result<IcHttpResponse, IcError> {
//!    Ok(IcHttpResponse {
//!      status: 200_u8.into(),
//!      ..Default::default()
//!    })
//! }
//!
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut service = ServiceBuilder::new()
//!   .layer(HttpConversionLayer)
//!   .service_fn(always_200_ok);
//!
//! let request = http::Request::post("https://internetcomputer.org")
//!   .max_response_bytes(42) //IC-specific concepts are added to the request as extensions.
//!   .header("Content-Type", "application/json")
//!   .body(vec![])
//!   .unwrap();
//!
//! let response = service.ready().await.unwrap().call(request).await.unwrap();
//!
//! assert_eq!(response.status(), http::StatusCode::OK);
//! # Ok(())
//! # }
//! ```
//!
//! [`IcHttpRequest`]: ic_cdk::api::management_canister::http_request::CanisterHttpRequestArgument
//! [`IcHttpResponse`]: ic_cdk::api::management_canister::http_request::HttpResponse

#[cfg(test)]
mod tests;

pub use request::{
    HttpRequest, HttpRequestConversionLayer, MaxResponseBytesRequestExtension,
    TransformContextRequestExtension,
};
pub use response::{HttpResponse, HttpResponseConversionLayer};

#[cfg(feature = "json")]
pub mod json;
mod request;
mod response;

use request::HttpRequestFilter;
use response::HttpResponseConversion;
use tower::Layer;

/// Middleware that combines [`HttpRequestConversionLayer`] to convert requests
/// and [`HttpResponseConversionLayer`] to convert responses to a [`Service`].
///
/// See the [module docs](crate::http) for an example.
///
/// [`Service`]: tower::Service
pub struct HttpConversionLayer;

impl<S> Layer<S> for HttpConversionLayer {
    type Service = HttpResponseConversion<tower::filter::Filter<S, HttpRequestFilter>>;

    fn layer(&self, inner: S) -> Self::Service {
        let stack =
            tower_layer::Stack::new(HttpRequestConversionLayer, HttpResponseConversionLayer);
        stack.layer(inner)
    }
}

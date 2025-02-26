//! HTTP translation layer

#[cfg(test)]
mod tests;

pub use request::{
    HttpRequest, HttpRequestConversionLayer, MaxResponseBytesRequestExtension,
    TransformContextRequestExtension,
};
pub use response::{HttpResponse, HttpResponseConversionLayer};

mod request;
mod response;

use request::HttpRequestFilter;
use response::HttpResponseConversion;
use tower::Layer;

pub struct HttpConversionLayer;

impl<S> Layer<S> for HttpConversionLayer {
    type Service = HttpResponseConversion<tower::filter::Filter<S, HttpRequestFilter>>;

    fn layer(&self, inner: S) -> Self::Service {
        let stack =
            tower_layer::Stack::new(HttpRequestConversionLayer, HttpResponseConversionLayer);
        stack.layer(inner)
    }
}

use ic_cdk::api::management_canister::http_request::{
    CanisterHttpRequestArgument as IcHttpRequest, HttpHeader as IcHttpHeader,
    HttpMethod as IcHttpMethod, TransformContext,
};
use thiserror::Error;
use tower::filter::Predicate;
use tower::BoxError;
use tower_layer::Layer;

pub type HttpRequest = http::Request<Vec<u8>>;

pub trait MaxResponseBytesRequestExtension: Sized {
    fn set_max_response_bytes(&mut self, value: u64);
    fn get_max_response_bytes(&self) -> Option<u64>;

    /// Convenience method to using the builder pattern.
    fn max_response_bytes(mut self, value: u64) -> Self {
        self.set_max_response_bytes(value);
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct MaxResponseBytesExtension(pub u64);

impl<T> MaxResponseBytesRequestExtension for http::Request<T> {
    fn set_max_response_bytes(&mut self, value: u64) {
        let extensions = self.extensions_mut();
        extensions.insert(MaxResponseBytesExtension(value));
    }

    fn get_max_response_bytes(&self) -> Option<u64> {
        self.extensions()
            .get::<MaxResponseBytesExtension>()
            .map(|e| e.0)
    }
}

impl MaxResponseBytesRequestExtension for http::request::Builder {
    fn set_max_response_bytes(&mut self, value: u64) {
        if let Some(extensions) = self.extensions_mut() {
            extensions.insert(MaxResponseBytesExtension(value));
        }
    }

    fn get_max_response_bytes(&self) -> Option<u64> {
        self.extensions_ref()
            .and_then(|extensions| extensions.get::<MaxResponseBytesExtension>().map(|e| e.0))
    }
}

pub trait TransformContextRequestExtension: Sized {
    fn set_transform_context(&mut self, value: TransformContext);
    fn get_transform_context(&self) -> Option<&TransformContext>;

    /// Convenience method to using the builder pattern.
    fn transform_context(mut self, value: TransformContext) -> Self {
        self.set_transform_context(value);
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TransformContextExtension(pub TransformContext);

impl<T> TransformContextRequestExtension for http::Request<T> {
    fn set_transform_context(&mut self, value: TransformContext) {
        let extensions = self.extensions_mut();
        extensions.insert(TransformContextExtension(value));
    }

    fn get_transform_context(&self) -> Option<&TransformContext> {
        self.extensions()
            .get::<TransformContextExtension>()
            .map(|e| &e.0)
    }
}

impl TransformContextRequestExtension for http::request::Builder {
    fn set_transform_context(&mut self, value: TransformContext) {
        if let Some(extensions) = self.extensions_mut() {
            extensions.insert(TransformContextExtension(value));
        }
    }

    fn get_transform_context(&self) -> Option<&TransformContext> {
        self.extensions_ref()
            .and_then(|extensions| extensions.get::<TransformContextExtension>().map(|e| &e.0))
    }
}

#[derive(Error, Clone, Debug, Eq, PartialEq)]
pub enum HttpRequestConversionError {
    #[error("HTTP method `{0}` is not supported")]
    UnsupportedHttpMethod(String),
    #[error("HTTP header `{name}` has an invalid value: {reason}")]
    InvalidHttpHeaderValue { name: String, reason: String },
}

fn try_map_http_request(request: HttpRequest) -> Result<IcHttpRequest, HttpRequestConversionError> {
    let url = request.uri().to_string();
    let max_response_bytes = request.get_max_response_bytes();
    let method = match request.method().as_str() {
        "GET" => IcHttpMethod::GET,
        "POST" => IcHttpMethod::POST,
        "HEAD" => IcHttpMethod::HEAD,
        unsupported => {
            return Err(HttpRequestConversionError::UnsupportedHttpMethod(
                unsupported.to_string(),
            ))
        }
    };
    let headers = request
        .headers()
        .iter()
        .map(|(header_name, header_value)| match header_value.to_str() {
            Ok(value) => Ok(IcHttpHeader {
                name: header_name.to_string(),
                value: value.to_string(),
            }),
            Err(e) => Err(HttpRequestConversionError::InvalidHttpHeaderValue {
                name: header_name.to_string(),
                reason: e.to_string(),
            }),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let transform = request.get_transform_context().cloned();
    let body = Some(request.into_body());
    Ok(IcHttpRequest {
        url,
        max_response_bytes,
        method,
        headers,
        body,
        transform,
    })
}

pub struct HttpRequestFilter;

impl Predicate<HttpRequest> for HttpRequestFilter {
    type Request = IcHttpRequest;

    fn check(&mut self, request: HttpRequest) -> Result<Self::Request, BoxError> {
        try_map_http_request(request).map_err(Into::into)
    }
}

pub struct HttpRequestConversionLayer;

impl<S> Layer<S> for HttpRequestConversionLayer {
    type Service = tower::filter::Filter<S, HttpRequestFilter>;

    fn layer(&self, inner: S) -> Self::Service {
        tower::filter::Filter::new(inner, HttpRequestFilter)
    }
}

//! HTTP translation layer

#[cfg(test)]
mod tests;

use crate::http::HttpResponseConversionError::{InvalidHttpHeaderName, InvalidHttpHeaderValue};
use http::{HeaderMap, HeaderName, HeaderValue, StatusCode};
use ic_cdk::api::management_canister::http_request::{
    CanisterHttpRequestArgument as IcHttpRequest, HttpHeader as IcHttpHeader,
    HttpMethod as IcHttpMethod, HttpResponse as IcHttpResponse, TransformContext,
};
use thiserror::Error;
use tower::{
    filter::Predicate,
    {BoxError, Layer},
};

pub type HttpRequest = http::Request<Vec<u8>>;

pub trait MaxResponseBytesRequestExtension {
    fn set_max_response_bytes(&mut self, value: u64);
    fn get_max_response_bytes(&self) -> Option<u64>;
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

/// Convenience trait to follow the builder pattern.
pub trait MaxResponseBytesRequestExtensionBuilder {
    /// See [`MaxResponseBytesRequestExtension::set_max_response_bytes`].
    fn max_response_bytes(self, value: u64) -> Self;
}

impl<T> MaxResponseBytesRequestExtensionBuilder for T
where
    T: MaxResponseBytesRequestExtension,
{
    fn max_response_bytes(mut self, value: u64) -> Self {
        self.set_max_response_bytes(value);
        self
    }
}

pub trait TransformContextRequestExtension {
    fn set_transform_context(&mut self, value: TransformContext);
    fn get_transform_context(&self) -> Option<&TransformContext>;
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

/// Convenience trait to follow the builder pattern.
pub trait TransformContextRequestExtensionBuilder {
    /// See [`TransformContextRequestExtension::set_transform_context`].
    fn transform_context(self, value: TransformContext) -> Self;
}

impl<T> TransformContextRequestExtensionBuilder for T
where
    T: TransformContextRequestExtension,
{
    fn transform_context(mut self, value: TransformContext) -> Self {
        self.set_transform_context(value);
        self
    }
}

#[derive(Error, Clone, Debug, Eq, PartialEq)]
pub enum HttpRequestFilterError {
    #[error("HTTP method `{0}` is not supported")]
    UnsupportedHttpMethod(String),
    #[error("HTTP header `{name}` has an invalid value: {reason}")]
    InvalidHttpHeaderValue { name: String, reason: String },
}

fn try_map_http_request(request: HttpRequest) -> Result<IcHttpRequest, HttpRequestFilterError> {
    let url = request.uri().to_string();
    let max_response_bytes = request.get_max_response_bytes();
    let method = match request.method().as_str() {
        "GET" => IcHttpMethod::GET,
        "POST" => IcHttpMethod::POST,
        "HEAD" => IcHttpMethod::HEAD,
        unsupported => {
            return Err(HttpRequestFilterError::UnsupportedHttpMethod(
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
            Err(e) => Err(HttpRequestFilterError::InvalidHttpHeaderValue {
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

pub type HttpResponse = http::Response<Vec<u8>>;

#[derive(Error, Debug)]
pub enum HttpResponseConversionError {
    #[error("Status code is invalid")]
    InvalidStatusCode,
    #[error("HTTP header `{name}` is invalid: {reason}")]
    InvalidHttpHeaderName { name: String, reason: String },
    #[error("HTTP header `{name}` has an invalid value: {reason}")]
    InvalidHttpHeaderValue { name: String, reason: String },
}

fn try_map_http_response(
    response: IcHttpResponse,
) -> Result<HttpResponse, HttpResponseConversionError> {
    let status = StatusCode::try_from(response.status.0.to_bytes_be().as_slice())
        .map_err(|_| HttpResponseConversionError::InvalidStatusCode)?;
    let mut builder = http::Response::builder().status(status);
    if let Some(headers) = builder.headers_mut() {
        let mut response_headers = HeaderMap::with_capacity(response.headers.len());
        for IcHttpHeader { name, value } in response.headers {
            response_headers.insert(
                HeaderName::try_from(&name).map_err(|e| InvalidHttpHeaderName {
                    name: name.clone(),
                    reason: e.to_string(),
                })?,
                HeaderValue::try_from(&value).map_err(|e| InvalidHttpHeaderValue {
                    name,
                    reason: e.to_string(),
                })?,
            );
        }
        headers.extend(response_headers);
    }

    Ok(builder
        .body(response.body)
        .expect("BUG: builder should have been modified only with validated data"))
}

pub struct HttpResponseConversionLayer;

pub struct HttpResponseConversion;

impl<S> Layer<S> for HttpResponseConversionLayer {
    type Service = tower::util::MapResult<S, HttpResponseConversion>;

    fn layer(&self, inner: S) -> Self::Service {
        tower::util::MapResult::new(inner, HttpResponseConversion)
    }
}

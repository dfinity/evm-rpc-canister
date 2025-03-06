use crate::convert::Convert;
use ic_cdk::api::management_canister::http_request::{
    CanisterHttpRequestArgument as IcHttpRequest, HttpHeader as IcHttpHeader,
    HttpMethod as IcHttpMethod, TransformContext,
};
use thiserror::Error;

/// HTTP request with a body made of bytes.
pub type HttpRequest = http::Request<Vec<u8>>;

/// Add support for max response bytes.
pub trait MaxResponseBytesRequestExtension: Sized {
    /// Set the max response bytes.
    ///
    /// If provided, the value must not exceed 2MB (2_000_000B).
    /// The call will be charged based on this parameter.
    /// If not provided, the maximum of 2MB will be used.
    fn set_max_response_bytes(&mut self, value: u64);

    /// Retrieves the current max response bytes value, if any.
    fn get_max_response_bytes(&self) -> Option<u64>;

    /// Convenience method to use the builder pattern.
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

/// Add support for transform context to specify how the response will be canonicalized by the replica
/// to maximize chances of consensus.
///
/// See the [docs](https://internetcomputer.org/docs/references/https-outcalls-how-it-works#transformation-function)
/// on HTTPs outcalls for more details.
pub trait TransformContextRequestExtension: Sized {
    /// Set the transform context.
    fn set_transform_context(&mut self, value: TransformContext);

    /// Retrieve the current transform context, if any.
    fn get_transform_context(&self) -> Option<&TransformContext>;

    /// Convenience method to use the builder pattern.
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

/// Error return when converting requests with [`HttpRequestConverter`].
#[derive(Error, Clone, Debug, Eq, PartialEq)]
pub enum HttpRequestConversionError {
    /// HTTP method is not supported
    #[error("HTTP method `{0}` is not supported")]
    UnsupportedHttpMethod(String),
    /// Header name is invalid.
    #[error("HTTP header `{name}` has an invalid value: {reason}")]
    InvalidHttpHeaderValue {
        /// Header name
        name: String,
        /// Reason for header value being invalid.
        reason: String,
    },
}

/// Convert requests of type [`HttpRequest`] into [`IcHttpRequest`].
#[derive(Clone, Debug)]
pub struct HttpRequestConverter;

impl Convert<HttpRequest> for HttpRequestConverter {
    type Output = IcHttpRequest;
    type Error = HttpRequestConversionError;

    fn try_convert(&mut self, request: HttpRequest) -> Result<Self::Output, Self::Error> {
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
}

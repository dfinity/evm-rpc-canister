use bytes::Bytes;
use http::Request;
use ic_cdk::api::management_canister::http_request::{
    CanisterHttpRequestArgument as IcHttpRequest, HttpHeader as IcHttpHeader,
    HttpMethod as IcHttpMethod, TransformContext,
};
use thiserror::Error;
use tower::filter::Predicate;
use tower::BoxError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IcHttpRequestWithCycles {
    pub request: IcHttpRequest,
    pub cycles: u128,
}

pub trait MaxResponseBytesRequestExtension {
    fn set_max_response_bytes(&mut self, value: u64);
    fn get_max_response_bytes(&self) -> Option<u64>;
}

impl MaxResponseBytesRequestExtension for IcHttpRequest {
    fn set_max_response_bytes(&mut self, value: u64) {
        self.max_response_bytes = Some(value);
    }

    fn get_max_response_bytes(&self) -> Option<u64> {
        self.max_response_bytes
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

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct HttpRequestFilter {}

#[derive(Error, Debug)]
pub enum HttpRequestFilterError {
    #[error("HTTP method `{0}` is not supported")]
    UnsupportedHttpMethod(String),
}

impl Predicate<http::Request<Bytes>> for HttpRequestFilter {
    type Request = IcHttpRequest;

    fn check(&mut self, request: Request<Bytes>) -> Result<Self::Request, BoxError> {
        let url = request.uri().to_string();
        let max_response_bytes = request.get_max_response_bytes();
        let method = match request.method().as_str() {
            "GET" => IcHttpMethod::GET,
            "POST" => IcHttpMethod::POST,
            "HEAD" => IcHttpMethod::HEAD,
            unsupported => {
                return Err(BoxError::from(
                    HttpRequestFilterError::UnsupportedHttpMethod(unsupported.to_string()),
                ))
            }
        };
        let headers = request
            .headers()
            .iter()
            .map(|(header_name, header_value)| IcHttpHeader {
                name: header_name.to_string(),
                value: header_value.to_str().unwrap().to_string(),
            })
            .collect();
        let body = Some(request.body().to_vec());
        let transform = request.get_transform_context().cloned();
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

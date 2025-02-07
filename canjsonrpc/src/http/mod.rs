use crate::client::{Client, HttpOutcallError};
use bytes::Bytes;
use http::header::CONTENT_TYPE;
use http::{HeaderMap, HeaderName, HeaderValue, Method};
use ic_cdk::api::management_canister::http_request::{
    CanisterHttpRequestArgument, HttpHeader, HttpMethod, HttpResponse, TransformContext,
};
use num_traits::ToPrimitive;
use serde::Serialize;
use thiserror::Error;
use url::Url;

#[must_use = "RequestBuilder does nothing until you 'send' it"]
pub struct RequestBuilder {
    client: Client,
    request: Result<CanisterHttpRequestArgument, RequestError>,
}

#[derive(Error, Debug)]
pub enum RequestError {
    #[error("URL is invalid (reason: {reason})")]
    InvalidUrl { reason: String },
    #[error("HTTP header is invalid (reason: {reason})")]
    InvalidHeader { reason: String },
    #[error("JSON body is invalid (reason: {reason})")]
    InvalidJson { reason: String },
}

pub enum ResponseError {}

impl RequestBuilder {
    pub fn new(client: Client, http_method: HttpMethod, url: &str) -> Self {
        let request = match Url::parse(url) {
            Ok(url) => Ok(CanisterHttpRequestArgument {
                url: url.to_string(),
                method: http_method,
                ..Default::default()
            }),
            Err(e) => Err(RequestError::InvalidUrl {
                reason: e.to_string(),
            }),
        };
        Self { client, request }
    }

    pub fn max_response_bytes(mut self, max_response_bytes: u64) -> Self {
        if let Ok(request) = self.request.as_mut() {
            request.max_response_bytes = Some(max_response_bytes)
        }
        self
    }

    pub fn transform_context(mut self, transform: TransformContext) -> Self {
        if let Ok(request) = self.request.as_mut() {
            request.transform = Some(transform);
        }
        self
    }

    /// Add a header to the request.
    ///
    /// The header name will be canonicalized.
    pub fn header(mut self, name: &str, value: &str) -> Self {
        if let Ok(request) = self.request.as_mut() {
            match (HeaderName::try_from(name), HeaderValue::try_from(value)) {
                (Ok(name), Ok(value)) => {
                    request.headers.push(HttpHeader {
                        name: name.to_string(),
                        value: value
                            .to_str()
                            .expect("BUG: header value was initially parsed from string")
                            .to_string(),
                    });
                }
                (Err(e), _) => {
                    self.request = Err(RequestError::InvalidHeader {
                        reason: e.to_string(),
                    })
                }
                (_, Err(e)) => {
                    self.request = Err(RequestError::InvalidHeader {
                        reason: e.to_string(),
                    })
                }
            }
        }
        self
    }

    pub fn json<T>(mut self, json: &T) -> Self
    where
        T: Serialize,
    {
        if let Ok(request) = self.request.as_mut() {
            match serde_json::to_vec(json) {
                Ok(body) => {
                    request.body = Some(body);
                    if !request
                        .headers
                        .iter()
                        .any(|header| header.name == CONTENT_TYPE.as_str())
                    {
                        request.headers.push(HttpHeader {
                            name: CONTENT_TYPE.to_string(),
                            value: "application/json".to_string(),
                        })
                    }
                }
                Err(e) => {
                    self.request = Err(RequestError::InvalidJson {
                        reason: e.to_string(),
                    })
                }
            }
        }
        self
    }

    // pub async fn send(self) -> Result<HttpResponse, HttpOutcallError> {
    //     match self.request {
    //         Ok(req) => self.client.execute_request(req).await,
    //         Err(err) => Err(HttpOutcallError::RequestError(err)),
    //     }
    // }
}

#[must_use = "JsonRpcRequestBuilder does nothing until you 'send' it"]
pub struct JsonRpcRequestBuilder {
    method: String,
    params: serde_json::Value,
    inner: RequestBuilder,
}

// impl JsonRpcRequestBuilder {
//     pub fn new<T>(client: Client, id: u64, method: &str, params: T, url: &str) -> Self
//     where
//         T: Serialize,
//     {
//         let request = JsonRpcRequest::new(id, method.to_string(), params);
//         let inner = RequestBuilder::new(client, HttpMethod::POST, url).json(&request);
//     }
// }

#[derive(Clone, Debug, PartialEq, Eq)]
struct MaxResponseBytesExtension(pub u64);

#[derive(Clone, Debug, PartialEq, Eq)]
struct TransformContextExtension(pub TransformContext);

pub trait AddMaxResponseBytesExtension {
    fn max_response_bytes(self, value: u64) -> Self;
}

impl AddMaxResponseBytesExtension for http::request::Builder {
    fn max_response_bytes(self, value: u64) -> Self {
        self.extension(MaxResponseBytesExtension(value))
    }
}

pub trait AddTransformContextExtension {
    fn transform_context(self, extension: TransformContext) -> Self;
}

impl AddTransformContextExtension for http::request::Builder {
    fn transform_context(self, extension: TransformContext) -> Self {
        self.extension(TransformContextExtension(extension))
    }
}

pub trait ReadMaxResponseBytesExtension {
    fn max_response_bytes(&self) -> Option<u64>;
}

impl<T> ReadMaxResponseBytesExtension for http::Request<T> {
    fn max_response_bytes(&self) -> Option<u64> {
        self.extensions()
            .get::<MaxResponseBytesExtension>()
            .map(|e| e.0)
    }
}

pub trait ReadTransformContextExtension {
    fn transform_context(&self) -> Option<&TransformContext>;
}

impl<T> ReadTransformContextExtension for http::Request<T> {
    fn transform_context(&self) -> Option<&TransformContext> {
        self.extensions()
            .get::<TransformContextExtension>()
            .map(|e| &e.0)
    }
}

pub trait IntoCanisterHttpRequest {
    fn into_canister_http_request(self) -> CanisterHttpRequestArgument;
}

//TODO: conversion is actually fallible
impl IntoCanisterHttpRequest for http::Request<Bytes> {
    fn into_canister_http_request(self) -> CanisterHttpRequestArgument {
        let url = self.uri().to_string();
        let max_response_bytes = self.max_response_bytes();
        let method = match self.method().as_str() {
            "GET" => HttpMethod::GET,
            "POST" => HttpMethod::POST,
            "HEAD" => HttpMethod::HEAD,
            _ => panic!("Unsupported HTTP method"),
        };
        let headers = self
            .headers()
            .iter()
            .map(|(header_name, header_value)| HttpHeader {
                name: header_name.to_string(),
                value: header_value.to_str().unwrap().to_string(),
            })
            .collect();
        let body = Some(self.body().to_vec());
        let transform = self.transform_context().cloned();
        CanisterHttpRequestArgument {
            url,
            max_response_bytes,
            method,
            headers,
            body,
            transform,
        }
    }
}

//TODO: conversion is actually fallible
pub fn convert_response(response: HttpResponse) -> http::Response<Bytes> {
    let mut builder = http::Response::builder()
        .status(response.status.0.to_u16().expect("valid HTTP status code"));
    if let Some(headers) = builder.headers_mut() {
        let mut response_headers = HeaderMap::with_capacity(response.headers.len());
        for HttpHeader { name, value } in response.headers {
            response_headers.insert(
                HeaderName::try_from(name).unwrap(),
                HeaderValue::try_from(value).unwrap(),
            );
        }
        headers.extend(response_headers);
    }

    builder.body(response.body.into()).unwrap()
}

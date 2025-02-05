use crate::client::{Client, HttpOutcallError};
use http::header::CONTENT_TYPE;
use http::{HeaderName, HeaderValue};
use ic_cdk::api::management_canister::http_request::{
    CanisterHttpRequestArgument, HttpHeader, HttpMethod, HttpResponse, TransformContext,
};
use serde::Serialize;
use url::Url;

#[must_use = "RequestBuilder does nothing until you 'send' it"]
pub struct RequestBuilder {
    client: Client,
    request: Result<CanisterHttpRequestArgument, RequestError>,
}

pub enum RequestError {
    InvalidUrl { reason: String },
    InvalidHeader { reason: String },
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

    pub async fn send(self) -> Result<HttpResponse, HttpOutcallError> {
        match self.request {
            Ok(req) => self.client.execute_request(req).await,
            Err(err) => Err(HttpOutcallError::RequestError(err)),
        }
    }
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

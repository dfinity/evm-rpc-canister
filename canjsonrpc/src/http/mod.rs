use http::header::CONTENT_TYPE;
use http::{HeaderName, HeaderValue};
use ic_cdk::api::management_canister::http_request::{
    CanisterHttpRequestArgument, HttpHeader, HttpMethod,
};
use serde::Serialize;
use url::Url;

pub struct RequestBuilder {
    request: Result<CanisterHttpRequestArgument, RequestError>,
}

pub enum RequestError {
    InvalidUrl { reason: String },
    InvalidHeader { reason: String },
    InvalidJson { reason: String },
}

impl RequestBuilder {
    pub fn new(http_method: HttpMethod, url: &str) -> Self {
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
        Self { request }
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
}

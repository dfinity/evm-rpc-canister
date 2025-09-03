use crate::mock_http_runtime::mock::CanisterHttpRequestMatcher;
use canhttp::http::json::{Id, JsonRpcRequest};
use pocket_ic::common::rest::{
    CanisterHttpHeader, CanisterHttpMethod, CanisterHttpReject, CanisterHttpReply,
    CanisterHttpRequest, CanisterHttpResponse,
};
use serde_json::Value;
use std::{collections::BTreeSet, str::FromStr};
use url::{Host, Url};

#[derive(Debug)]
pub struct JsonRpcRequestMatcher {
    pub method: String,
    pub id: Option<Id>,
    pub params: Option<Value>,
    pub url: Option<String>,
    pub host: Option<Host>,
    pub request_headers: Option<Vec<CanisterHttpHeader>>,
    pub max_response_bytes: Option<u64>,
}

impl JsonRpcRequestMatcher {
    pub fn with_method(method: impl Into<String>) -> Self {
        Self {
            method: method.into(),
            id: None,
            params: None,
            url: None,
            host: None,
            request_headers: None,
            max_response_bytes: None,
        }
    }

    pub fn with_params(self, params: impl Into<Value>) -> Self {
        Self {
            params: Some(params.into()),
            ..self
        }
    }

    pub fn with_id(self, id: impl Into<Id>) -> Self {
        Self {
            id: Some(id.into()),
            ..self
        }
    }

    pub fn with_max_response_bytes(self, max_response_bytes: impl Into<u64>) -> Self {
        Self {
            max_response_bytes: Some(max_response_bytes.into()),
            ..self
        }
    }

    pub fn request_body(&self) -> JsonRpcRequest<Value> {
        let mut request_body =
            JsonRpcRequest::new(&self.method, self.params.clone().unwrap_or(Value::Null));
        if let Some(id) = &self.id {
            request_body.set_id(id.clone());
        }
        request_body
    }
}

impl CanisterHttpRequestMatcher for JsonRpcRequestMatcher {
    fn assert_matches(&self, request: &CanisterHttpRequest) {
        let req_url = Url::from_str(&request.url).expect("BUG: invalid URL");
        if let Some(ref url) = self.url {
            let mock_url = Url::from_str(url).unwrap();
            assert_eq!(mock_url, req_url);
        }
        if let Some(ref host) = self.host {
            assert_eq!(
                host,
                &req_url.host().expect("BUG: missing host in URL").to_owned()
            );
        }
        assert_eq!(CanisterHttpMethod::POST, request.http_method);
        if let Some(ref headers) = self.request_headers {
            // Header names are case-insensitive
            fn lower_case_header_name(
                CanisterHttpHeader { name, value }: &CanisterHttpHeader,
            ) -> CanisterHttpHeader {
                CanisterHttpHeader {
                    name: name.to_lowercase(),
                    value: value.clone(),
                }
            }
            assert_eq!(
                headers
                    .iter()
                    .map(lower_case_header_name)
                    .collect::<BTreeSet<_>>(),
                request
                    .headers
                    .iter()
                    .map(lower_case_header_name)
                    .collect::<BTreeSet<_>>()
            );
        }
        let actual_body: JsonRpcRequest<Value> =
            serde_json::from_slice(&request.body).expect("BUG: failed to parse JSON request body");
        assert_eq!(self.request_body(), actual_body);
        if let Some(max_response_bytes) = self.max_response_bytes {
            assert_eq!(Some(max_response_bytes), request.max_response_bytes);
        }
    }
}

pub enum JsonRpcResponse {
    CanisterHttpReply {
        status: u16,
        headers: Vec<CanisterHttpHeader>,
        body: Value,
    },
    CanisterHttpReject {
        reject_code: u64,
        message: String,
    },
}

impl From<Value> for JsonRpcResponse {
    fn from(body: Value) -> Self {
        Self::CanisterHttpReply {
            status: 200,
            headers: vec![],
            body,
        }
    }
}

impl From<&Value> for JsonRpcResponse {
    fn from(body: &Value) -> Self {
        Self::from(body.clone())
    }
}

impl From<String> for JsonRpcResponse {
    fn from(body: String) -> Self {
        Self::from(Value::from_str(&body).expect("BUG: invalid JSON-RPC response"))
    }
}

impl From<&str> for JsonRpcResponse {
    fn from(body: &str) -> Self {
        Self::from(body.to_string())
    }
}

impl From<JsonRpcResponse> for CanisterHttpResponse {
    fn from(response: JsonRpcResponse) -> Self {
        match response {
            JsonRpcResponse::CanisterHttpReply {
                status,
                headers,
                body,
            } => CanisterHttpResponse::CanisterHttpReply(CanisterHttpReply {
                status,
                headers,
                body: serde_json::to_vec(&body).unwrap(),
            }),
            JsonRpcResponse::CanisterHttpReject {
                reject_code,
                message,
            } => CanisterHttpResponse::CanisterHttpReject(CanisterHttpReject {
                reject_code,
                message,
            }),
        }
    }
}

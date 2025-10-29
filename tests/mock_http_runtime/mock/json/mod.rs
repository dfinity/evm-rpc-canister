#[cfg(test)]
mod tests;

use crate::mock_http_runtime::mock::CanisterHttpRequestMatcher;
use canhttp::http::json::{ConstantSizeId, Id, JsonRpcRequest};
use derive_more::{From, Into};
use pocket_ic::common::rest::{
    CanisterHttpHeader, CanisterHttpMethod, CanisterHttpReply, CanisterHttpRequest,
    CanisterHttpResponse,
};
use serde_json::{json, Value};
use std::{collections::BTreeSet, str::FromStr};
use url::{Host, Url};

#[derive(From, Into)]
pub struct JsonRpcId(Id);

impl From<u64> for JsonRpcId {
    fn from(id: u64) -> Self {
        Self(Id::from(ConstantSizeId::from(id)))
    }
}

#[derive(Clone, Debug)]
pub struct JsonRpcRequestMatcher {
    pub method: String,
    pub id: Option<Id>,
    pub params: Option<Value>,
    pub url: Option<Url>,
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

    pub fn with_id(self, id: impl Into<JsonRpcId>) -> Self {
        Self {
            id: Some(Id::from(id.into())),
            ..self
        }
    }

    pub fn with_params(self, params: impl Into<Value>) -> Self {
        Self {
            params: Some(params.into()),
            ..self
        }
    }

    pub fn with_url(self, url: &str) -> Self {
        Self {
            url: Some(Url::parse(url).expect("BUG: invalid URL")),
            ..self
        }
    }

    pub fn with_host(self, host: &str) -> Self {
        Self {
            host: Some(Host::parse(host).expect("BUG: invalid host for a URL")),
            ..self
        }
    }

    pub fn with_request_headers(self, headers: Vec<(impl ToString, impl ToString)>) -> Self {
        Self {
            request_headers: Some(
                headers
                    .into_iter()
                    .map(|(name, value)| CanisterHttpHeader {
                        name: name.to_string(),
                        value: value.to_string(),
                    })
                    .collect(),
            ),
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
        serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "method": &self.method,
            "params": self.params.clone().unwrap_or(Value::Null),
            "id": self.id.clone().unwrap_or(Id::Null),
        }))
        .unwrap()
    }
}

impl CanisterHttpRequestMatcher for JsonRpcRequestMatcher {
    fn matches(&self, request: &CanisterHttpRequest) -> bool {
        let req_url = Url::from_str(&request.url).expect("BUG: invalid URL");
        if let Some(ref mock_url) = self.url {
            if mock_url != &req_url {
                return false;
            }
        }
        if let Some(ref host) = self.host {
            match req_url.host() {
                Some(ref req_host) if req_host == host => {}
                _ => return false,
            }
        }
        if CanisterHttpMethod::POST != request.http_method {
            return false;
        }
        if let Some(ref headers) = self.request_headers {
            fn lower_case_header_name(
                CanisterHttpHeader { name, value }: &CanisterHttpHeader,
            ) -> CanisterHttpHeader {
                CanisterHttpHeader {
                    name: name.to_lowercase(),
                    value: value.clone(),
                }
            }
            let expected: BTreeSet<_> = headers.iter().map(lower_case_header_name).collect();
            let actual: BTreeSet<_> = request.headers.iter().map(lower_case_header_name).collect();
            if expected != actual {
                return false;
            }
        }
        match serde_json::from_slice(&request.body) {
            Ok(actual_body) => {
                if self.request_body() != actual_body {
                    return false;
                }
            }
            // Not a JSON-RPC request
            Err(_) => return false,
        }
        if let Some(max_response_bytes) = self.max_response_bytes {
            if Some(max_response_bytes) != request.max_response_bytes {
                return false;
            }
        }
        true
    }
}

#[derive(Clone)]
pub struct JsonRpcResponse {
    pub status: u16,
    pub headers: Vec<CanisterHttpHeader>,
    pub body: Value,
}

impl From<Value> for JsonRpcResponse {
    fn from(body: Value) -> Self {
        Self {
            status: 200,
            headers: vec![],
            body,
        }
    }
}

impl JsonRpcResponse {
    pub fn with_id(mut self, id: impl Into<JsonRpcId>) -> JsonRpcResponse {
        self.body["id"] =
            serde_json::to_value(Id::from(id.into())).expect("BUG: cannot serialize ID");
        self
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
        CanisterHttpResponse::CanisterHttpReply(CanisterHttpReply {
            status: response.status,
            headers: response.headers,
            body: serde_json::to_vec(&response.body).unwrap(),
        })
    }
}

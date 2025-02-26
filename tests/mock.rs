use ic_cdk::api::call::RejectionCode;
use pocket_ic::common::rest::{
    CanisterHttpHeader, CanisterHttpMethod, CanisterHttpReject, CanisterHttpReply,
    CanisterHttpRequest, CanisterHttpResponse,
};
use std::collections::BTreeSet;
use std::str::FromStr;
use url::Url;

pub struct MockOutcallBody(pub Vec<u8>);

impl From<String> for MockOutcallBody {
    fn from(string: String) -> Self {
        string.as_bytes().to_vec().into()
    }
}
impl<'a> From<&'a str> for MockOutcallBody {
    fn from(string: &'a str) -> Self {
        string.to_string().into()
    }
}
impl From<Vec<u8>> for MockOutcallBody {
    fn from(bytes: Vec<u8>) -> Self {
        MockOutcallBody(bytes)
    }
}

#[derive(Clone, Debug)]
pub struct MockOutcallBuilder(MockOutcall);

impl MockOutcallBuilder {
    pub fn new(status: u16, body: impl Into<MockOutcallBody>) -> Self {
        Self(MockOutcall {
            method: None,
            url: None,
            request_headers: None,
            request_body: None,
            max_response_bytes: None,
            response: CanisterHttpResponse::CanisterHttpReply(CanisterHttpReply {
                status,
                headers: vec![],
                body: body.into().0,
            }),
        })
    }

    pub fn new_error(code: RejectionCode, message: impl ToString) -> Self {
        Self(MockOutcall {
            method: None,
            url: None,
            request_headers: None,
            request_body: None,
            max_response_bytes: None,
            response: CanisterHttpResponse::CanisterHttpReject(CanisterHttpReject {
                reject_code: code as u64,
                message: message.to_string(),
            }),
        })
    }

    pub fn with_method(mut self, method: CanisterHttpMethod) -> Self {
        self.0.method = Some(method);
        self
    }

    pub fn with_url(mut self, url: impl ToString) -> Self {
        self.0.url = Some(url.to_string());
        self
    }

    pub fn with_request_headers(mut self, headers: Vec<(impl ToString, impl ToString)>) -> Self {
        self.0.request_headers = Some(
            headers
                .into_iter()
                .map(|(name, value)| CanisterHttpHeader {
                    name: name.to_string(),
                    value: value.to_string(),
                })
                .collect(),
        );
        self
    }

    pub fn with_raw_request_body(self, body: &str) -> Self {
        self.with_request_body(MockJsonRequestBody::from_raw_request_unchecked(body))
    }

    pub fn with_request_body(mut self, body: impl Into<MockJsonRequestBody>) -> Self {
        self.0.request_body = Some(body.into());
        self
    }

    pub fn with_max_response_bytes(mut self, max_response_bytes: u64) -> Self {
        self.0.max_response_bytes = Some(max_response_bytes);
        self
    }

    pub fn build(self) -> MockOutcall {
        self.0
    }
}

impl From<MockOutcallBuilder> for MockOutcall {
    fn from(builder: MockOutcallBuilder) -> Self {
        builder.build()
    }
}

#[derive(Clone, Debug)]
pub struct MockOutcall {
    pub method: Option<CanisterHttpMethod>,
    pub url: Option<String>,
    pub request_headers: Option<Vec<CanisterHttpHeader>>,
    pub request_body: Option<MockJsonRequestBody>,
    pub max_response_bytes: Option<u64>,
    pub response: CanisterHttpResponse,
}

impl MockOutcall {
    pub fn assert_matches(&self, request: &CanisterHttpRequest) {
        if let Some(ref url) = self.url {
            let mock_url = Url::from_str(url).unwrap();
            let req_url = Url::from_str(&request.url).unwrap();
            assert_eq!(mock_url, req_url);
        }
        if let Some(ref method) = self.method {
            assert_eq!(method, &request.http_method);
        }
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
        if let Some(ref expected_body) = self.request_body {
            let actual_body: serde_json::Value = serde_json::from_slice(&request.body)
                .expect("BUG: failed to parse JSON request body");
            expected_body.assert_matches(&actual_body);
        }
        if let Some(max_response_bytes) = self.max_response_bytes {
            assert_eq!(Some(max_response_bytes), request.max_response_bytes);
        }
    }
}

/// Assertions on parts of the JSON-RPC request body.
#[derive(Clone, Debug)]
pub struct MockJsonRequestBody {
    pub jsonrpc: String,
    pub method: String,
    pub id: Option<u64>,
    pub params: Option<serde_json::Value>,
}

impl MockJsonRequestBody {
    pub fn new(method: impl ToString) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            method: method.to_string(),
            id: None,
            params: None,
        }
    }

    pub fn builder(method: impl ToString) -> MockJsonRequestBuilder {
        MockJsonRequestBuilder(Self::new(method))
    }

    pub fn from_raw_request_unchecked(raw_request: &str) -> Self {
        let request: serde_json::Value =
            serde_json::from_str(raw_request).expect("BUG: failed to parse JSON request");
        Self {
            jsonrpc: request["jsonrpc"]
                .as_str()
                .expect("BUG: missing jsonrpc field")
                .to_string(),
            method: request["method"]
                .as_str()
                .expect("BUG: missing method field")
                .to_string(),
            id: request["id"].as_u64(),
            params: request.get("params").cloned(),
        }
    }

    pub fn assert_matches(&self, request_body: &serde_json::Value) {
        assert_eq!(
            self.jsonrpc,
            request_body["jsonrpc"]
                .as_str()
                .expect("BUG: missing jsonrpc field")
        );
        assert_eq!(
            self.method,
            request_body["method"]
                .as_str()
                .expect("BUG: missing method field")
        );
        if let Some(id) = self.id {
            assert_eq!(
                id,
                request_body["id"].as_u64().expect("BUG: missing id field")
            );
        }
        if let Some(expected_params) = &self.params {
            assert_eq!(
                expected_params,
                request_body
                    .get("params")
                    .expect("BUG: missing params field")
            );
        }
    }
}

#[derive(Clone, Debug)]
pub struct MockJsonRequestBuilder(MockJsonRequestBody);

impl MockJsonRequestBuilder {
    pub fn with_params(mut self, params: impl Into<serde_json::Value>) -> Self {
        self.0.params = Some(params.into());
        self
    }

    pub fn build(self) -> MockJsonRequestBody {
        self.0
    }
}

impl From<MockJsonRequestBuilder> for MockJsonRequestBody {
    fn from(builder: MockJsonRequestBuilder) -> Self {
        builder.build()
    }
}

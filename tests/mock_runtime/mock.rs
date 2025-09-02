use canhttp::http::json::{Id, JsonRpcRequest};
use dyn_clone::DynClone;
use pocket_ic::common::rest::{
    CanisterHttpHeader, CanisterHttpMethod, CanisterHttpReply, CanisterHttpRequest,
    CanisterHttpResponse,
};
use serde_json::Value;
use std::collections::{BTreeSet, VecDeque};
use std::fmt::Debug;
use std::str::FromStr;
use url::{Host, Url};

#[derive(Clone, Debug, Default)]
pub struct MockHttpOutcalls(VecDeque<MockHttpOutcall>);

impl MockHttpOutcalls {
    pub fn push(&mut self, mock: MockHttpOutcall) {
        self.0.push_back(mock);
    }
}

impl Iterator for MockHttpOutcalls {
    type Item = MockHttpOutcall;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.pop_front()
    }
}

#[derive(Clone, Debug)]
pub struct MockHttpOutcall {
    pub request: Box<dyn CanisterHttpRequestMatcher>,
    pub response: CanisterHttpResponse,
}

#[derive(Clone, Debug, Default)]
pub struct MockHttpOutcallsBuilder(MockHttpOutcalls);

impl MockHttpOutcallsBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn given(
        self,
        request: impl CanisterHttpRequestMatcher + 'static,
    ) -> MockJsonRpcOutcallBuilder {
        MockJsonRpcOutcallBuilder::new(self, Box::new(request))
    }

    pub fn build(self) -> MockHttpOutcalls {
        self.0
    }
}

impl From<MockHttpOutcallsBuilder> for MockHttpOutcalls {
    fn from(builder: MockHttpOutcallsBuilder) -> Self {
        builder.build()
    }
}

pub struct MockJsonRpcOutcallBuilder(MockHttpOutcallsBuilder, Box<dyn CanisterHttpRequestMatcher>);

impl MockJsonRpcOutcallBuilder {
    pub fn new(
        parent: MockHttpOutcallsBuilder,
        request: Box<dyn CanisterHttpRequestMatcher>,
    ) -> Self {
        Self(parent, request)
    }

    pub fn respond_with(mut self, response: CanisterHttpResponse) -> MockHttpOutcallsBuilder {
        self.0 .0.push(MockHttpOutcall {
            request: self.1,
            response,
        });
        self.0
    }

    pub fn respond_with_success(self, body: &Value) -> MockHttpOutcallsBuilder {
        self.respond_with(CanisterHttpResponse::CanisterHttpReply(CanisterHttpReply {
            status: 200,
            headers: vec![],
            body: serde_json::to_vec(&body).unwrap(),
        }))
    }
}

pub trait CanisterHttpRequestMatcher: Send + DynClone + Debug {
    fn assert_matches(&self, request: &CanisterHttpRequest);
}
dyn_clone::clone_trait_object!(CanisterHttpRequestMatcher);

#[derive(Clone, Debug)]
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

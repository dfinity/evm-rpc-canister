use canhttp::http::json::JsonRpcRequest;
use dyn_clone::DynClone;
use ic_cdk::api::call::RejectionCode;
use pocket_ic::common::rest::{
    CanisterHttpHeader, CanisterHttpMethod, CanisterHttpReject, CanisterHttpReply,
    CanisterHttpRequest, CanisterHttpResponse,
};
use serde_json::Value;
use std::fmt::Debug;
use std::{
    collections::{BTreeSet, VecDeque},
    iter,
    str::FromStr,
};
use url::{Host, Url};

#[derive(Clone, Default)]
pub struct MockOutcallQueue(VecDeque<Box<dyn MockOutcallIterator>>);

trait MockOutcallIterator: Iterator<Item = MockOutcall> + Send + DynClone {}
dyn_clone::clone_trait_object!(MockOutcallIterator);

impl<T> MockOutcallIterator for T where T: Iterator<Item = MockOutcall> + Clone + Send + 'static {}

impl MockOutcallQueue {
    pub fn push(&mut self, outcall: impl Into<MockOutcall>, repeat: MockOutcallRepeat) {
        self.0.push_back(match repeat {
            MockOutcallRepeat::Once => Box::new(iter::once(outcall.into())),
            MockOutcallRepeat::Times(n) => Box::new(iter::repeat_n(outcall.into(), n)),
            MockOutcallRepeat::Forever => Box::new(iter::repeat(outcall.into())),
        })
    }
}

impl Iterator for MockOutcallQueue {
    type Item = MockOutcall;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(iter) = self.0.front_mut() {
            match iter.next() {
                Some(item) => return Some(item),
                None => {
                    self.0.pop_front();
                }
            }
        }
        None
    }
}

#[derive(Clone, Default)]
pub enum MockOutcallRepeat {
    #[default]
    Once,
    Times(usize),
    Forever,
}

pub fn once() -> MockOutcallRepeat {
    MockOutcallRepeat::Once
}

pub fn times(n: usize) -> MockOutcallRepeat {
    MockOutcallRepeat::Times(n)
}

pub fn forever() -> MockOutcallRepeat {
    MockOutcallRepeat::Forever
}

pub struct MockOutcallBody(pub Vec<u8>);

impl From<&Value> for MockOutcallBody {
    fn from(value: &Value) -> Self {
        value.to_string().into()
    }
}

impl From<Value> for MockOutcallBody {
    fn from(value: Value) -> Self {
        Self::from(serde_json::to_vec(&value).unwrap())
    }
}

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
    pub fn new(responses: impl IntoIterator<Item = CanisterHttpResponse>) -> Self {
        Self(MockOutcall {
            method: None,
            url: None,
            host: None,
            request_headers: None,
            request_body: None,
            max_response_bytes: None,
            responses: responses.into_iter().collect(),
        })
    }

    pub fn new_success(bodies: impl IntoIterator<Item = impl Into<MockOutcallBody>>) -> Self {
        MockOutcallBuilder::new(bodies.into_iter().map(|body| {
            CanisterHttpResponse::CanisterHttpReply(CanisterHttpReply {
                status: 200,
                headers: vec![],
                body: body.into().0,
            })
        }))
    }

    pub fn new_reject(code: RejectionCode, num_providers: usize, message: impl ToString) -> Self {
        MockOutcallBuilder::new(vec![
            CanisterHttpResponse::CanisterHttpReject(
                CanisterHttpReject {
                    reject_code: code as u64,
                    message: message.to_string(),
                }
            );
            num_providers
        ])
    }

    pub fn with_method(mut self, method: CanisterHttpMethod) -> Self {
        self.0.method = Some(method);
        self
    }

    pub fn with_url(mut self, url: impl ToString) -> Self {
        self.0.url = Some(url.to_string());
        self
    }

    pub fn with_host(mut self, host: &str) -> Self {
        self.0.host = Some(Host::parse(host).expect("BUG: invalid host for a URL"));
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
        self.with_request_body(serde_json::from_str(body).unwrap())
    }

    pub fn with_request_body(mut self, body: Value) -> Self {
        self.0.request_body = Some(serde_json::from_value(body).unwrap());
        self
    }

    pub fn with_max_response_bytes(mut self, max_response_bytes: u64) -> Self {
        self.0.max_response_bytes = Some(max_response_bytes);
        self
    }

    pub fn with_sequential_response_ids(mut self, first_id: u64) -> Self{
        for (i, response) in self.0.responses.iter_mut().enumerate() {
            if let CanisterHttpResponse::CanisterHttpReply(reply) = response {
                let mut body: Value = serde_json::from_slice(&reply.body).expect("Invalid response body");
                *body.get_mut("id").expect("Missing request ID") = Value::Number((first_id + i as u64).into());
                reply.body = serde_json::to_vec(&body).expect("Invalid response body");
            }
        }
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
    pub host: Option<Host>,
    pub request_headers: Option<Vec<CanisterHttpHeader>>,
    pub request_body: Option<JsonRpcRequest<Value>>,
    pub max_response_bytes: Option<u64>,
    pub responses: Vec<CanisterHttpResponse>,
}

impl MockOutcall {
    pub fn assert_matches(&self, request: &CanisterHttpRequest) {
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
        if let Some(ref method) = self.method {
            assert_eq!(method, &request.http_method);
        }
        if let Some(ref headers) = self.request_headers {
            assert_eq!(
                headers.iter().collect::<BTreeSet<_>>(),
                request.headers.iter().collect::<BTreeSet<_>>()
            );
        }
        if let Some(ref expected_body) = self.request_body {
            let actual_body: JsonRpcRequest<Value> = serde_json::from_slice(&request.body)
                .expect("BUG: failed to parse JSON request body");
            assert_eq!(expected_body, &actual_body);
        }
        if let Some(max_response_bytes) = self.max_response_bytes {
            assert_eq!(Some(max_response_bytes), request.max_response_bytes);
        }
    }
}

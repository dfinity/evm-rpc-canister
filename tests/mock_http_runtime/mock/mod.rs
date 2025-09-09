use pocket_ic::common::rest::{CanisterHttpRequest, CanisterHttpResponse};
use serde_json::Value;
use std::fmt::Debug;

pub mod json;

#[derive(Debug, Default)]
pub struct MockHttpOutcalls(Vec<MockHttpOutcall>);

impl MockHttpOutcalls {
    pub const NEVER: MockHttpOutcalls = Self(Vec::new());

    pub fn push(&mut self, mock: MockHttpOutcall) {
        self.0.push(mock);
    }

    pub fn pop_matching(&mut self, request: &CanisterHttpRequest) -> Option<MockHttpOutcall> {
        let matching_positions = self
            .0
            .iter()
            .enumerate()
            .filter_map(|(i, mock)| {
                if mock.request.matches(request) {
                    Some(i)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        match matching_positions.len() {
            0 => None,
            1 => Some(self.0.swap_remove(matching_positions[0])),
            _ => panic!("Multiple mocks match the request: {:?}", request),
        }
    }
}

impl Drop for MockHttpOutcalls {
    fn drop(&mut self) {
        if !self.0.is_empty() {
            panic!(
                "MockHttpOutcalls dropped but {} mocks were not consumed: {:?}",
                self.0.len(),
                self.0
            );
        }
    }
}

#[derive(Debug)]
#[must_use]
pub struct MockHttpOutcall {
    pub request: Box<dyn CanisterHttpRequestMatcher>,
    pub response: CanisterHttpResponse,
}

#[derive(Debug, Default)]
pub struct MockHttpOutcallsBuilder(MockHttpOutcalls);

impl MockHttpOutcallsBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn given(
        self,
        request: impl CanisterHttpRequestMatcher + 'static,
    ) -> MockHttpOutcallBuilder {
        MockHttpOutcallBuilder {
            parent: self,
            request: Box::new(request),
        }
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

#[must_use]
pub struct MockHttpOutcallBuilder {
    parent: MockHttpOutcallsBuilder,
    request: Box<dyn CanisterHttpRequestMatcher>,
}

impl MockHttpOutcallBuilder {
    pub fn respond_with(
        mut self,
        response: impl Into<CanisterHttpResponse>,
    ) -> MockHttpOutcallsBuilder {
        self.parent.0.push(MockHttpOutcall {
            request: self.request,
            response: response.into(),
        });
        self.parent
    }
}

pub trait CanisterHttpRequestMatcher: Send + Debug {
    fn matches(&self, request: &CanisterHttpRequest) -> bool;
}

pub struct CanisterHttpReply(pocket_ic::common::rest::CanisterHttpReply);

impl CanisterHttpReply {
    pub fn with_status(status: u16) -> Self {
        Self(pocket_ic::common::rest::CanisterHttpReply {
            status,
            headers: vec![],
            body: vec![],
        })
    }

    pub fn with_body(mut self, body: impl Into<Value>) -> Self {
        self.0.body = serde_json::to_vec(&body.into()).unwrap();
        self
    }
}

impl From<CanisterHttpReply> for CanisterHttpResponse {
    fn from(value: CanisterHttpReply) -> Self {
        CanisterHttpResponse::CanisterHttpReply(value.0)
    }
}

pub struct CanisterHttpReject(pocket_ic::common::rest::CanisterHttpReject);

impl CanisterHttpReject {
    pub fn with_reject_code(reject_code: ic_error_types::RejectCode) -> Self {
        Self(pocket_ic::common::rest::CanisterHttpReject {
            reject_code: reject_code as u64,
            message: "".to_string(),
        })
    }

    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.0.message = message.into();
        self
    }
}

impl From<CanisterHttpReject> for CanisterHttpResponse {
    fn from(value: CanisterHttpReject) -> Self {
        CanisterHttpResponse::CanisterHttpReject(value.0)
    }
}

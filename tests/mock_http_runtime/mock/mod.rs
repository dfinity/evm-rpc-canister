use pocket_ic::common::rest::{CanisterHttpRequest, CanisterHttpResponse};
use std::fmt::Debug;

pub mod json;

#[derive(Debug, Default)]
pub struct MockHttpOutcalls(Vec<MockHttpOutcall>);

impl MockHttpOutcalls {
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
            1 => Some(self.0.remove(matching_positions[0])),
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

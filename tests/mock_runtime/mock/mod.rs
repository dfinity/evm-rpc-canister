use dyn_clone::DynClone;
use pocket_ic::common::rest::{CanisterHttpRequest, CanisterHttpResponse};
use std::{collections::VecDeque, fmt::Debug};

pub mod json;

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

    pub fn respond_with(
        mut self,
        response: impl Into<CanisterHttpResponse>,
    ) -> MockHttpOutcallsBuilder {
        self.0 .0.push(MockHttpOutcall {
            request: self.1,
            response: response.into(),
        });
        self.0
    }
}

pub trait CanisterHttpRequestMatcher: Send + DynClone + Debug {
    fn assert_matches(&self, request: &CanisterHttpRequest);
}
dyn_clone::clone_trait_object!(CanisterHttpRequestMatcher);

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
#[must_use]
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
struct MockHttpOutcallBuilder {
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

pub trait CanisterHttpRequestMatcher: Send + DynClone + Debug {
    fn assert_matches(&self, request: &CanisterHttpRequest);
}
dyn_clone::clone_trait_object!(CanisterHttpRequestMatcher);

//! Simple types to create basic unit tests for the [`crate::EvmRpcClient`].
//!
//! Types and methods for this module are only available for non-canister architecture (non `wasm32`).

use crate::{ClientBuilder, IcError, Runtime};
use async_trait::async_trait;
use candid::{utils::ArgumentEncoder, CandidType, Decode, Encode, Principal};
use serde::de::DeserializeOwned;
use std::collections::BTreeMap;

impl<R, C> ClientBuilder<R, C> {
    /// Set the runtime to a [`StubRuntime`].
    pub fn with_stub_responses(self) -> ClientBuilder<StubRuntime, C> {
        self.with_runtime(|_runtime| StubRuntime::default())
    }

    /// Change the runtime to return the given stub response for all calls.
    pub fn with_default_stub_response<Out: CandidType>(
        self,
        stub_response: Out,
    ) -> ClientBuilder<StubRuntime, C> {
        self.with_stub_responses()
            .with_default_response(stub_response)
    }
}

impl<C> ClientBuilder<StubRuntime, C> {
    /// Change the runtime to return the given stub response for all calls.
    pub fn with_default_response<Out: CandidType>(
        self,
        stub_response: Out,
    ) -> ClientBuilder<StubRuntime, C> {
        self.with_runtime(|runtime| runtime.with_default_response(stub_response))
    }

    /// Change the runtime to return the given stub response for calls to the given method.
    pub fn with_response_for_method<Out: CandidType>(
        self,
        method_name: &str,
        stub_response: Out,
    ) -> ClientBuilder<StubRuntime, C> {
        self.with_runtime(|runtime| runtime.with_response_for_method(method_name, stub_response))
    }
}

/// An implementation of [`Runtime`] that always returns the same candid-encoded response
/// for a given method.
///
/// Implement your own [`Runtime`] in case a more refined approach is needed.
pub struct StubRuntime {
    default_call_result: Option<Vec<u8>>,
    method_to_call_result_map: BTreeMap<String, Vec<u8>>,
}

impl StubRuntime {
    /// Create a new [`StubRuntime`] with the given default stub response.
    pub fn new() -> Self {
        Self {
            default_call_result: None,
            method_to_call_result_map: BTreeMap::new(),
        }
    }

    /// Create a new [`StubRuntime`] with the given default stub response.
    pub fn with_default_response<Out: CandidType>(mut self, stub_response: Out) -> Self {
        let result = Encode!(&stub_response).expect("Failed to encode Candid stub response");
        self.default_call_result = Some(result);
        self
    }

    /// Modify a [`StubRuntime`] to return the given response for the given method
    pub fn with_response_for_method<Out: CandidType>(
        mut self,
        method: &str,
        stub_response: Out,
    ) -> Self {
        self.method_to_call_result_map.insert(
            method.to_string(),
            Encode!(&stub_response).expect("Failed to encode Candid stub response"),
        );
        self
    }

    fn call<Out>(&self, method: &str) -> Result<Out, IcError>
    where
        Out: CandidType + DeserializeOwned,
    {
        let bytes = self
            .method_to_call_result_map
            .get(method)
            .or(self.default_call_result.as_ref())
            .unwrap_or_else(|| panic!("No available call response value for method `{method}`"));
        Ok(Decode!(bytes, Out).expect("Failed to decode Candid stub response"))
    }
}

impl Default for StubRuntime {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Runtime for StubRuntime {
    async fn update_call<In, Out>(
        &self,
        _id: Principal,
        method: &str,
        _args: In,
        _cycles: u128,
    ) -> Result<Out, IcError>
    where
        In: ArgumentEncoder + Send,
        Out: CandidType + DeserializeOwned,
    {
        self.call(method)
    }

    async fn query_call<In, Out>(
        &self,
        _id: Principal,
        method: &str,
        _args: In,
    ) -> Result<Out, IcError>
    where
        In: ArgumentEncoder + Send,
        Out: CandidType + DeserializeOwned,
    {
        self.call(method)
    }
}

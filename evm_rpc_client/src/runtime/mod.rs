#[cfg(feature = "pocket-ic")]
#[allow(missing_docs)]
mod pocket_ic;

use async_trait::async_trait;
use candid::utils::ArgumentEncoder;
use candid::{CandidType, Principal};
use ic_error_types::RejectCode;
#[cfg(feature = "pocket-ic")]
pub use pocket_ic::{
    forever, once, MockOutcall, MockOutcallBody, MockOutcallBuilder, MockOutcallQueue,
    MockOutcallRepeat, PocketIcRuntime, RepeatExt,
};
use serde::de::DeserializeOwned;

/// Abstract the canister runtime so that the client code can be reused:
/// * in production using `ic_cdk`,
/// * in unit tests by mocking this trait,
/// * in integration tests by implementing this trait for `PocketIc`.
#[async_trait]
pub trait Runtime {
    /// Defines how asynchronous inter-canister update calls are made.
    async fn update_call<In, Out>(
        &self,
        id: Principal,
        method: &str,
        args: In,
        cycles: u128,
    ) -> Result<Out, (RejectCode, String)>
    where
        In: ArgumentEncoder + Send,
        Out: CandidType + DeserializeOwned;

    /// Defines how asynchronous inter-canister query calls are made.
    async fn query_call<In, Out>(
        &self,
        id: Principal,
        method: &str,
        args: In,
    ) -> Result<Out, (RejectCode, String)>
    where
        In: ArgumentEncoder + Send,
        Out: CandidType + DeserializeOwned;
}

/// Runtime when interacting with a canister running on the Internet Computer.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct IcRuntime;

#[async_trait]
impl Runtime for IcRuntime {
    async fn update_call<In, Out>(
        &self,
        id: Principal,
        method: &str,
        args: In,
        cycles: u128,
    ) -> Result<Out, (RejectCode, String)>
    where
        In: ArgumentEncoder + Send,
        Out: CandidType + DeserializeOwned,
    {
        ic_cdk::api::call::call_with_payment128(id, method, args, cycles)
            .await
            .map(|(res,)| res)
            .map_err(|(code, message)| (crate::convert_reject_code(code), message))
    }

    async fn query_call<In, Out>(
        &self,
        id: Principal,
        method: &str,
        args: In,
    ) -> Result<Out, (RejectCode, String)>
    where
        In: ArgumentEncoder + Send,
        Out: CandidType + DeserializeOwned,
    {
        ic_cdk::api::call::call(id, method, args)
            .await
            .map(|(res,)| res)
            .map_err(|(code, message)| (crate::convert_reject_code(code), message))
    }
}

use async_trait::async_trait;
use candid::utils::ArgumentEncoder;
use candid::{CandidType, Principal};
use ic_cdk::call::{Call, CallFailed, CallRejected, CandidDecodeFailed};
use ic_error_types::RejectCode;
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
    ) -> Result<Out, CallFailed>
    where
        In: ArgumentEncoder + Send,
        Out: CandidType + DeserializeOwned;

    /// Defines how asynchronous inter-canister query calls are made.
    async fn query_call<In, Out>(
        &self,
        id: Principal,
        method: &str,
        args: In,
    ) -> Result<Out, CallFailed>
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
    ) -> Result<Out, CallFailed>
    where
        In: ArgumentEncoder + Send,
        Out: CandidType + DeserializeOwned,
    {
        Call::unbounded_wait(id, method)
            .with_cycles(cycles)
            .with_args(&args)
            .await
            .and_then(|response| {
                response
                    .candid::<Out>()
                    .map_err(decode_error_to_call_failed)
            })
    }

    async fn query_call<In, Out>(
        &self,
        id: Principal,
        method: &str,
        args: In,
    ) -> Result<Out, CallFailed>
    where
        In: ArgumentEncoder + Send,
        Out: CandidType + DeserializeOwned,
    {
        Call::unbounded_wait(id, method)
            .with_args(&args)
            .await
            .and_then(|response| {
                response
                    .candid::<Out>()
                    .map_err(decode_error_to_call_failed)
            })
    }
}

fn decode_error_to_call_failed(err: CandidDecodeFailed) -> CallFailed {
    CallFailed::CallRejected(CallRejected::with_rejection(
        RejectCode::CanisterError as u32,
        err.to_string(),
    ))
}

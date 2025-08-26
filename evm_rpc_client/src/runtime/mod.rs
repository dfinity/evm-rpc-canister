#[cfg(feature = "pocket-ic")]
mod pocket_ic;

use async_trait::async_trait;
use candid::utils::ArgumentEncoder;
use candid::{CandidType, Principal};
use ic_cdk::api::call::RejectionCode as IcCdkRejectionCode;
use ic_error_types::RejectCode;
#[cfg(feature = "pocket-ic")]
pub use pocket_ic::{
    forever, once, times, MockOutcall, MockOutcallBody, MockOutcallBuilder, MockOutcallQueue,
    MockOutcallRepeat, PocketIcRuntime,
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
            .map_err(|(code, message)| (convert_reject_code(code), message))
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
            .map_err(|(code, message)| (convert_reject_code(code), message))
    }
}

fn convert_reject_code(code: IcCdkRejectionCode) -> RejectCode {
    match code {
        IcCdkRejectionCode::SysFatal => RejectCode::SysFatal,
        IcCdkRejectionCode::SysTransient => RejectCode::SysTransient,
        IcCdkRejectionCode::DestinationInvalid => RejectCode::DestinationInvalid,
        IcCdkRejectionCode::CanisterReject => RejectCode::CanisterReject,
        IcCdkRejectionCode::CanisterError => RejectCode::CanisterError,
        IcCdkRejectionCode::Unknown => {
            // This can only happen if there is a new error code on ICP that the CDK is not aware of.
            // We map it to SysFatal since none of the other error codes apply.
            // In particular, note that RejectCode::SysUnknown is only applicable to inter-canister
            // calls that used ic0.call_with_best_effort_response.
            RejectCode::SysFatal
        }
        IcCdkRejectionCode::NoError => {
            unreachable!("inter-canister calls should never produce a RejectionCode::NoError error")
        }
    }
}

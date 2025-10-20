use async_trait::async_trait;
use candid::{utils::ArgumentEncoder, CandidType, Principal};
use ic_cdk::call::{Call, CallFailed, CandidDecodeFailed};
use ic_error_types::RejectCode;
use serde::de::DeserializeOwned;
use thiserror::Error;

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
    ) -> Result<Out, IcError>
    where
        In: ArgumentEncoder + Send,
        Out: CandidType + DeserializeOwned;

    /// Defines how asynchronous inter-canister query calls are made.
    async fn query_call<In, Out>(
        &self,
        id: Principal,
        method: &str,
        args: In,
    ) -> Result<Out, IcError>
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
    ) -> Result<Out, IcError>
    where
        In: ArgumentEncoder + Send,
        Out: CandidType + DeserializeOwned,
    {
        Call::unbounded_wait(id, method)
            .with_args(&args)
            .with_cycles(cycles)
            .await
            .map_err(IcError::from)
            .and_then(|response| response.candid::<Out>().map_err(IcError::from))
    }

    async fn query_call<In, Out>(
        &self,
        id: Principal,
        method: &str,
        args: In,
    ) -> Result<Out, IcError>
    where
        In: ArgumentEncoder + Send,
        Out: CandidType + DeserializeOwned,
    {
        Call::unbounded_wait(id, method)
            .with_args(&args)
            .await
            .map_err(IcError::from)
            .and_then(|response| response.candid::<Out>().map_err(IcError::from))
    }
}

/// Error returned by the Internet Computer when making an inter-canister call.
#[derive(Error, Clone, Debug, PartialEq, Eq)]
pub enum IcError {
    /// The liquid cycle balance is insufficient to perform the call.
    #[error("Insufficient liquid cycles balance, available: {available}, required: {required}")]
    InsufficientLiquidCycleBalance {
        /// The liquid cycle balance available in the canister.
        available: u128,
        /// The required cycles to perform the call.
        required: u128,
    },

    /// The `ic0.call_perform` operation failed when performing the inter-canister call.
    #[error("Inter-canister call perform failed")]
    CallPerformFailed,

    /// The inter-canister call is rejected.
    #[error("Inter-canister call rejected: {code:?} - {message})")]
    CallRejected {
        /// Rejection code as specified [here](https://internetcomputer.org/docs/current/references/ic-interface-spec#reject-codes)
        code: RejectCode,
        /// Associated helper message.
        message: String,
    },

    /// The response from the inter-canister call could not be decoded as Candid.
    #[error("The inter-canister call response could not be decoded: {message}")]
    CandidDecodeFailed {
        /// The specific Candid error that occurred.
        message: String,
    },
}

impl From<CallFailed> for IcError {
    fn from(err: CallFailed) -> Self {
        match err {
            CallFailed::CallPerformFailed(_) => IcError::CallPerformFailed,
            CallFailed::CallRejected(e) => {
                IcError::CallRejected {
                    // `CallRejected::reject_code()` can only return an error result if there is a
                    // new error code on ICP that the CDK is not aware of. We map it to `SysFatal`
                    // since none of the other error codes apply.
                    // In particular, note that `RejectCode::SysUnknown` is only applicable to
                    // inter-canister calls that used `ic0.call_with_best_effort_response`.
                    code: e.reject_code().unwrap_or(RejectCode::SysFatal),
                    message: e.reject_message().to_string(),
                }
            }
            CallFailed::InsufficientLiquidCycleBalance(e) => {
                IcError::InsufficientLiquidCycleBalance {
                    available: e.available,
                    required: e.required,
                }
            }
        }
    }
}

impl From<CandidDecodeFailed> for IcError {
    fn from(err: CandidDecodeFailed) -> Self {
        IcError::CandidDecodeFailed {
            message: err.to_string(),
        }
    }
}

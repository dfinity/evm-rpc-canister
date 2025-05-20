#[cfg(test)]
mod tests;

use crate::RpcService;
use candid::{CandidType, Deserialize};
use std::fmt::Debug;
use thiserror::Error;

pub type RpcResult<T> = Result<T, RpcError>;

#[derive(Clone, Debug, Eq, PartialEq, CandidType, Deserialize)]
pub enum MultiRpcResult<T> {
    Consistent(RpcResult<T>),
    Inconsistent(Vec<(RpcService, RpcResult<T>)>),
}

impl<T> MultiRpcResult<T> {
    pub fn map<R>(self, mut f: impl FnMut(T) -> R) -> MultiRpcResult<R> {
        match self {
            MultiRpcResult::Consistent(result) => MultiRpcResult::Consistent(result.map(f)),
            MultiRpcResult::Inconsistent(results) => MultiRpcResult::Inconsistent(
                results
                    .into_iter()
                    .map(|(service, result)| {
                        (
                            service,
                            match result {
                                Ok(ok) => Ok(f(ok)),
                                Err(err) => Err(err),
                            },
                        )
                    })
                    .collect(),
            ),
        }
    }
}

impl<T: Debug> MultiRpcResult<T> {
    pub fn expect_consistent(self) -> RpcResult<T> {
        match self {
            MultiRpcResult::Consistent(result) => result,
            MultiRpcResult::Inconsistent(inconsistent_result) => {
                panic!("Expected consistent, but got: {:?}", inconsistent_result)
            }
        }
    }

    pub fn expect_inconsistent(self) -> Vec<(RpcService, RpcResult<T>)> {
        match self {
            MultiRpcResult::Consistent(consistent_result) => {
                panic!("Expected inconsistent:, but got: {:?}", consistent_result)
            }
            MultiRpcResult::Inconsistent(results) => results,
        }
    }
}

impl<T> From<RpcResult<T>> for MultiRpcResult<T> {
    fn from(result: RpcResult<T>) -> Self {
        MultiRpcResult::Consistent(result)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, CandidType, Deserialize, Error)]
pub enum RpcError {
    #[error("Provider error: {0}")]
    ProviderError(ProviderError),
    #[error("HTTP outcall error: {0}")]
    HttpOutcallError(HttpOutcallError),
    #[error("JSON-RPC error: {0}")]
    JsonRpcError(JsonRpcError),
    #[error("Validation error: {0}")]
    ValidationError(ValidationError),
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, CandidType, Deserialize, Error)]
pub enum ProviderError {
    #[error("No permission to call this provider")]
    NoPermission,
    #[error("Not enough cycles, expected {expected}, received {received}")]
    TooFewCycles { expected: u128, received: u128 },
    #[error("Provider not found")]
    ProviderNotFound,
    #[error("Missing required provider")]
    MissingRequiredProvider,
    #[error("Invalid RPC config: {0}")]
    InvalidRpcConfig(String),
}

/// An "outdated" definition of rejection code.
///
/// This implementation was [copied](https://github.com/dfinity/cdk-rs/blob/83ba5fc7b3316a6fa4e7f704b689c95c9e677029/src/ic-cdk/src/api/call.rs#L21) from ic-cdk v0.17.
///
/// The `ic_cdk::api::call::RejectionCode` type is deprecated since ic-cdk v0.18.
/// The replacement `ic_cdk::call::RejectCode` re-exports the type defined in the `ic-error-types` crate.
/// We can not simply switch to the replacement because the existing `RejectionCode` is a public type in evm_rpc canister's interface.
/// To maintain compatibility, we retain the "outdated" definition here.
///
/// The `canhttp::client::IcError` converts the error procuded by ic-cdk inter-canister calls into this type.
#[repr(usize)]
#[derive(CandidType, Deserialize, Clone, Copy, Hash, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RejectionCode {
    NoError = 0,

    SysFatal = 1,
    SysTransient = 2,
    DestinationInvalid = 3,
    CanisterReject = 4,
    CanisterError = 5,

    Unknown,
}

impl From<usize> for RejectionCode {
    fn from(code: usize) -> Self {
        match code {
            0 => RejectionCode::NoError,
            1 => RejectionCode::SysFatal,
            2 => RejectionCode::SysTransient,
            3 => RejectionCode::DestinationInvalid,
            4 => RejectionCode::CanisterReject,
            5 => RejectionCode::CanisterError,
            _ => RejectionCode::Unknown,
        }
    }
}

impl From<u32> for RejectionCode {
    fn from(code: u32) -> Self {
        RejectionCode::from(code as usize)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord, CandidType, Deserialize, Error)]
pub enum HttpOutcallError {
    /// Error from the IC system API.
    #[error("IC error (code: {code:?}): {message}")]
    IcError {
        code: RejectionCode,
        message: String,
    },
    /// Response is not a valid JSON-RPC response,
    /// which means that the response was not successful (status other than 2xx)
    /// or that the response body could not be deserialized into a JSON-RPC response.
    #[error("Invalid HTTP JSON-RPC response: status {status}, body: {body}, parsing error: {parsing_error:?}")]
    InvalidHttpJsonRpcResponse {
        status: u16,
        body: String,
        #[serde(rename = "parsingError")]
        parsing_error: Option<String>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord, CandidType, Deserialize, Error)]
#[error("JSON-RPC error (code: {code}): {message}")]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
}

#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord, CandidType, Deserialize, Error)]
pub enum ValidationError {
    #[error("Custom: {0}")]
    Custom(String),
    #[error("Invalid hex: {0}")]
    InvalidHex(String),
}

impl From<ProviderError> for RpcError {
    fn from(err: ProviderError) -> Self {
        RpcError::ProviderError(err)
    }
}

impl From<HttpOutcallError> for RpcError {
    fn from(err: HttpOutcallError) -> Self {
        RpcError::HttpOutcallError(err)
    }
}

impl From<JsonRpcError> for RpcError {
    fn from(err: JsonRpcError) -> Self {
        RpcError::JsonRpcError(err)
    }
}

impl From<ValidationError> for RpcError {
    fn from(err: ValidationError) -> Self {
        RpcError::ValidationError(err)
    }
}

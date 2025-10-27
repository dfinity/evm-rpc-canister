use evm_rpc_types::{MultiRpcResult, ProviderError, RpcError, RpcResult};

/// The retry strategy when performing calls to the EVM RPC canister.
#[derive(Clone, Eq, PartialEq, Debug, Default)]
pub enum EvmRpcRetryStrategy {
    /// Do not perform any retries.
    #[default]
    NoRetry,

    /// If a call fails due to insufficient cycles, try again with double the cycles until the
    /// maximum number of allowed retries has been performed.
    DoubleCycles {
        /// The maximum number of retries to perform.
        max_num_retries: u32,
    },
}

pub trait EvmRpcResult {
    fn is_too_few_cycles_error(&self) -> bool;
}

impl<T> EvmRpcResult for MultiRpcResult<T> {
    fn is_too_few_cycles_error(&self) -> bool {
        match self {
            MultiRpcResult::Consistent(result) => result.is_too_few_cycles_error(),
            MultiRpcResult::Inconsistent(results) => results
                .iter()
                .any(|(_, result)| result.is_too_few_cycles_error()),
        }
    }
}

impl<T> EvmRpcResult for RpcResult<T> {
    fn is_too_few_cycles_error(&self) -> bool {
        match self {
            Err(err) => matches!(
                err,
                RpcError::ProviderError(ProviderError::TooFewCycles { .. })
            ),
            _ => false,
        }
    }
}

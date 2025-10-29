use crate::{request::Request, IcError};
use evm_rpc_types::{MultiRpcResult, ProviderError, RpcError, RpcResult};

/// A retry policy for the [`EvmRpcClient`].
///
/// [`EvmRpcClient`]: crate::EvmRpcClient
pub trait EvmRpcRetryPolicy<Config, Params, CandidOutput, Output> {
    /// If the request should be retried, this method returns an optional containing the (possibly
    /// mutated) request to retry.
    /// If the request should not be retried, it returns [`None`].
    fn retry(
        &mut self,
        request: &mut Request<Config, Params, CandidOutput, Output>,
        result: &mut Result<Output, IcError>,
    ) -> Option<Request<Config, Params, CandidOutput, Output>>;

    /// Clones a request before sending it.
    /// If the request cannot be cloned, or if it does not need to be (e.g., if the policy never
    /// retries), this method returns [`None`].
    fn clone_request(
        &mut self,
        request: &Request<Config, Params, CandidOutput, Output>,
    ) -> Option<Request<Config, Params, CandidOutput, Output>>;
}

/// Never perform any retries.
#[derive(Debug, Clone)]
pub struct NoRetry;

impl<Config, Params, CandidOutput, Output> EvmRpcRetryPolicy<Config, Params, CandidOutput, Output>
    for NoRetry
{
    fn retry(
        &mut self,
        _request: &mut Request<Config, Params, CandidOutput, Output>,
        _result: &mut Result<Output, IcError>,
    ) -> Option<Request<Config, Params, CandidOutput, Output>> {
        None
    }

    fn clone_request(
        &mut self,
        _request: &Request<Config, Params, CandidOutput, Output>,
    ) -> Option<Request<Config, Params, CandidOutput, Output>> {
        None
    }
}

/// Retry strategy where the request is re-tried with double the cycles when it fails due to a
/// [ProviderError::TooFewCycles] error.
#[derive(Debug, Clone)]
pub struct DoubleCycles {
    /// The remaining number of retries.
    num_retries: u32,
}

impl DoubleCycles {
    /// Create a [`DoubleCycles`] policy with the given maximum number of retries.
    pub fn with_max_num_retries(max_num_retries: u32) -> Self {
        DoubleCycles {
            num_retries: max_num_retries,
        }
    }
}

impl<Config, Params, CandidOutput, Output>
    EvmRpcRetryPolicy<Config, Params, CandidOutput, MultiRpcResult<Output>> for DoubleCycles
where
    Request<Config, Params, CandidOutput, MultiRpcResult<Output>>: Clone,
{
    fn retry(
        &mut self,
        request: &mut Request<Config, Params, CandidOutput, MultiRpcResult<Output>>,
        result: &mut Result<MultiRpcResult<Output>, IcError>,
    ) -> Option<Request<Config, Params, CandidOutput, MultiRpcResult<Output>>> {
        match result {
            Ok(result) if result.is_too_few_cycles_error() => {
                if self.num_retries > 0 {
                    self.num_retries = self.num_retries.saturating_sub(1);
                    request.cycles = request.cycles.saturating_mul(2);
                    Some(request.clone())
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn clone_request(
        &mut self,
        request: &Request<Config, Params, CandidOutput, MultiRpcResult<Output>>,
    ) -> Option<Request<Config, Params, CandidOutput, MultiRpcResult<Output>>> {
        Some(request.clone())
    }
}

trait IsTooFewCyclesError {
    fn is_too_few_cycles_error(&self) -> bool;
}

impl<T> IsTooFewCyclesError for MultiRpcResult<T> {
    fn is_too_few_cycles_error(&self) -> bool {
        match self {
            MultiRpcResult::Consistent(result) => result.is_too_few_cycles_error(),
            MultiRpcResult::Inconsistent(results) => results
                .iter()
                .any(|(_, result)| result.is_too_few_cycles_error()),
        }
    }
}

impl<T> IsTooFewCyclesError for RpcResult<T> {
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

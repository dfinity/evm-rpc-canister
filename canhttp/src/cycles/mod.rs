#[cfg(test)]
mod tests;

use crate::client::IcHttpRequestWithCycles;
use ic_cdk::api::management_canister::http_request::CanisterHttpRequestArgument;
use thiserror::Error;
use tower::filter::Predicate;
use tower::BoxError;

/// Estimate the amount of cycles needed for a single HTTPs outcall.
pub trait EstimateRequestCyclesCost {
    /// Estimate the amount of cycles to attach to an HTTPs outcall.
    ///
    /// The returned amount should be at least the value specified [here](https://internetcomputer.org/docs/current/developer-docs/gas-cost#https-outcalls),
    /// otherwise the call will be rejected by the Internet Computer.
    /// The minimum value is computed by [`DefaultRequestCyclesCostEstimator`].
    fn cycles_to_attach(&self, request: &CanisterHttpRequestArgument) -> u128;

    /// Estimate the amount of cycles to charge the caller.
    ///
    /// If the value is `None`, no cycles will be charged.
    fn cycles_to_charge(
        &self,
        _request: &CanisterHttpRequestArgument,
        _attached_cycles: u128,
    ) -> Option<u128> {
        None
    }
}

/// Estimate the exact minimum cycles amount required to send an HTTPs outcall as specified
/// [here](https://internetcomputer.org/docs/current/developer-docs/gas-cost#https-outcalls).
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DefaultRequestCyclesCostEstimator {
    num_nodes_in_subnet: u32,
}

impl DefaultRequestCyclesCostEstimator {
    /// Maximum value for `max_response_bytes` which is 2MB,
    /// see the [IC specification](https://internetcomputer.org/docs/current/references/ic-interface-spec#ic-http_request).
    pub const DEFAULT_MAX_RESPONSE_BYTES: u64 = 2_000_000;

    /// Create a new estimator for a subnet having the given number of nodes.
    pub const fn new(num_nodes_in_subnet: u32) -> Self {
        DefaultRequestCyclesCostEstimator {
            num_nodes_in_subnet,
        }
    }

    fn base_fee(&self) -> u128 {
        3_000_000_u128
            .saturating_add(60_000_u128.saturating_mul(self.num_nodes_as_u128()))
            .saturating_mul(self.num_nodes_as_u128())
    }

    fn request_fee(&self, bytes: u128) -> u128 {
        400_u128
            .saturating_mul(self.num_nodes_as_u128())
            .saturating_mul(bytes)
    }

    fn response_fee(&self, bytes: u128) -> u128 {
        800_u128
            .saturating_mul(self.num_nodes_as_u128())
            .saturating_mul(bytes)
    }

    fn num_nodes_as_u128(&self) -> u128 {
        self.num_nodes_in_subnet as u128
    }
}

impl EstimateRequestCyclesCost for DefaultRequestCyclesCostEstimator {
    fn cycles_to_attach(&self, request: &CanisterHttpRequestArgument) -> u128 {
        let payload_body_bytes = request
            .body
            .as_ref()
            .map(|body| body.len())
            .unwrap_or_default();
        let extra_payload_bytes = request.url.len()
            + request
                .headers
                .iter()
                .map(|header| header.name.len() + header.value.len())
                .sum::<usize>()
            + request.transform.as_ref().map_or(0, |transform| {
                transform.function.0.method.len() + transform.context.len()
            });
        let max_response_bytes = request
            .max_response_bytes
            .unwrap_or(Self::DEFAULT_MAX_RESPONSE_BYTES);
        let request_bytes = (payload_body_bytes + extra_payload_bytes) as u128;
        self.base_fee()
            + self.request_fee(request_bytes)
            + self.response_fee(max_response_bytes as u128)
    }
}

/// Error return by the [`CyclesAccounting] middleware.
#[derive(Error, Debug)]
pub enum CyclesAccountingError {
    /// Error returned when the caller should be charge but did not attach sufficiently many cycles.
    #[error("insufficient cycles (expected {expected:?}, received {received:?})")]
    InsufficientCyclesError {
        /// Expected amount of cycles. Minimum value that should have been sent.
        expected: u128,
        /// Received amount of cycles
        received: u128,
    },
}

/// A middleware to handle cycles accounting.
/// How cycles are estimated is given by `CyclesEstimator`
#[derive(Clone, Debug)]
pub struct CyclesAccounting<CyclesEstimator> {
    cycles_estimator: CyclesEstimator,
}

impl<CyclesEstimator> CyclesAccounting<CyclesEstimator> {
    /// Create a new middleware given the cycles estimator.
    pub fn new(cycles_estimator: CyclesEstimator) -> Self {
        Self { cycles_estimator }
    }
}

impl<CyclesEstimator> Predicate<CanisterHttpRequestArgument> for CyclesAccounting<CyclesEstimator>
where
    CyclesEstimator: EstimateRequestCyclesCost,
{
    type Request = IcHttpRequestWithCycles;

    fn check(&mut self, request: CanisterHttpRequestArgument) -> Result<Self::Request, BoxError> {
        let cycles_to_attach = self.cycles_estimator.cycles_to_attach(&request);
        if let Some(cycles_to_charge) = self
            .cycles_estimator
            .cycles_to_charge(&request, cycles_to_attach)
        {
            let cycles_available = ic_cdk::api::call::msg_cycles_available128();
            if cycles_available < cycles_to_charge {
                return Err(Box::new(CyclesAccountingError::InsufficientCyclesError {
                    expected: cycles_to_charge,
                    received: cycles_available,
                }));
            }
            assert_eq!(
                ic_cdk::api::call::msg_cycles_accept128(cycles_to_charge),
                cycles_to_charge
            );
        }
        Ok(IcHttpRequestWithCycles {
            request,
            cycles: cycles_to_attach,
        })
    }
}

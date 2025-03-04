#[cfg(test)]
mod tests;

use crate::client::IcHttpRequestWithCycles;
use crate::convert::Convert;
use ic_cdk::api::management_canister::http_request::CanisterHttpRequestArgument;
use thiserror::Error;

/// Estimate the amount of cycles to charge for a single HTTPs outcall.
pub trait CyclesChargingPolicy {
    /// Determine the amount of cycles to charge the caller.
    ///
    /// If the value is `0`, no cycles will be charged, meaning that the canister using that library will
    /// pay for HTTPs outcalls with its own cycles. Otherwise, the returned amount of cycles will be transferred
    /// from the caller to the canister's cycles balance to pay (in part or fully) for the HTTPs outcall.
    fn cycles_to_charge(
        &self,
        _request: &CanisterHttpRequestArgument,
        _attached_cycles: u128,
    ) -> u128 {
        0
    }
}

/// Estimate the exact minimum cycles amount required to send an HTTPs outcall as specified
/// [here](https://internetcomputer.org/docs/current/developer-docs/gas-cost#https-outcalls).
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CyclesCostEstimator {
    num_nodes_in_subnet: u32,
}

impl CyclesCostEstimator {
    /// Maximum value for `max_response_bytes` which is 2MB,
    /// see the [IC specification](https://internetcomputer.org/docs/current/references/ic-interface-spec#ic-http_request).
    pub const DEFAULT_MAX_RESPONSE_BYTES: u64 = 2_000_000;

    /// Create a new estimator for a subnet having the given number of nodes.
    pub const fn new(num_nodes_in_subnet: u32) -> Self {
        CyclesCostEstimator {
            num_nodes_in_subnet,
        }
    }

    /// Compute the number of cycles required to send the given request via HTTPs outcall.
    ///
    /// An HTTP outcall entails calling the `http_request` method on the management canister interface,
    /// which requires that cycles to pay for the call must be explicitly attached with the call
    /// ([IC specification](https://internetcomputer.org/docs/current/references/ic-interface-spec#ic-http_request)).
    /// The required amount of cycles to attach is specified
    /// [here](https://internetcomputer.org/docs/current/developer-docs/gas-cost#https-outcalls).
    pub fn cost_of_http_request(&self, request: &CanisterHttpRequestArgument) -> u128 {
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

/// Error return by the [`CyclesAccounting`] middleware.
#[derive(Error, Clone, Debug, PartialEq, Eq)]
pub enum CyclesAccountingError {
    /// Error returned when the caller should be charged but did not attach sufficiently many cycles.
    #[error("insufficient cycles (expected {expected:?}, received {received:?})")]
    InsufficientCyclesError {
        /// Expected amount of cycles. Minimum value that should have been sent.
        expected: u128,
        /// Received amount of cycles
        received: u128,
    },
}

/// A middleware to handle cycles accounting, i.e. verify if sufficiently many cycles are available in a request.
/// How cycles are estimated is given by `CyclesEstimator`
#[derive(Clone, Debug)]
pub struct CyclesAccounting<Charging> {
    cycles_cost_estimator: CyclesCostEstimator,
    charging_policy: Charging,
}

impl<Charging> CyclesAccounting<Charging> {
    /// Create a new middleware given the cycles estimator.
    pub fn new(num_nodes_in_subnet: u32, charging_policy: Charging) -> Self {
        Self {
            cycles_cost_estimator: CyclesCostEstimator::new(num_nodes_in_subnet),
            charging_policy,
        }
    }
}

impl<CyclesEstimator> Convert<CanisterHttpRequestArgument> for CyclesAccounting<CyclesEstimator>
where
    CyclesEstimator: CyclesChargingPolicy,
{
    type Output = IcHttpRequestWithCycles;
    type Error = CyclesAccountingError;

    fn try_convert(
        &mut self,
        request: CanisterHttpRequestArgument,
    ) -> Result<Self::Output, Self::Error> {
        let cycles_to_attach = self.cycles_cost_estimator.cost_of_http_request(&request);
        let cycles_to_charge = self
            .charging_policy
            .cycles_to_charge(&request, cycles_to_attach);
        if cycles_to_charge > 0 {
            let cycles_available = ic_cdk::api::call::msg_cycles_available128();
            if cycles_available < cycles_to_charge {
                return Err(CyclesAccountingError::InsufficientCyclesError {
                    expected: cycles_to_charge,
                    received: cycles_available,
                });
            }
            let cycles_received = ic_cdk::api::call::msg_cycles_accept128(cycles_to_charge);
            assert_eq!(
                cycles_received, cycles_to_charge,
                "Expected to receive {cycles_to_charge}, but got {cycles_received}"
            );
        }
        Ok(IcHttpRequestWithCycles {
            request,
            cycles: cycles_to_attach,
        })
    }
}

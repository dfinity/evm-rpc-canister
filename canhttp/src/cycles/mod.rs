#[cfg(test)]
mod tests;

use ic_cdk::api::management_canister::http_request::CanisterHttpRequestArgument;
use thiserror::Error;
use tower::filter::Predicate;
use tower::BoxError;

pub trait EstimateRequestCyclesCost {
    /// Estimate cycle cost of an HTTPs outcall.
    fn cycles_cost(&self, request: &CanisterHttpRequestArgument) -> u128;
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DefaultRequestCyclesCostEstimator {
    num_nodes_in_subnet: u32,
}

impl DefaultRequestCyclesCostEstimator {
    pub const DEFAULT_MAX_RESPONSE_BYTES: u64 = 2_000_000;

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
    fn cycles_cost(&self, request: &CanisterHttpRequestArgument) -> u128 {
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

/// Charge estimated request cycles cost to the caller.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ChargeCaller<E> {
    estimator: E,
}

impl<E> ChargeCaller<E> {
    pub fn new(estimator: E) -> Self {
        Self { estimator }
    }
}

impl<E> Predicate<CanisterHttpRequestArgument> for ChargeCaller<E>
where
    E: EstimateRequestCyclesCost,
{
    type Request = CanisterHttpRequestArgument;

    fn check(&mut self, request: CanisterHttpRequestArgument) -> Result<Self::Request, BoxError> {
        let cycles_cost = self.estimator.cycles_cost(&request);
        let cycles_available = ic_cdk::api::call::msg_cycles_available128();
        if cycles_available < cycles_cost {
            return Err(Box::new(ChargeCallerError::InsufficientCyclesError {
                expected: cycles_cost,
                received: cycles_available,
            }));
        }
        assert_eq!(
            ic_cdk::api::call::msg_cycles_accept128(cycles_cost),
            cycles_cost
        );
        Ok(request)
    }
}

#[derive(Error, Debug)]
pub enum ChargeCallerError {
    #[error("insufficient cycles (expected {expected:?}, received {received:?})")]
    InsufficientCyclesError { expected: u128, received: u128 },
}

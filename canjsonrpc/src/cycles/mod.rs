use ic_cdk::api::management_canister::http_request::CanisterHttpRequestArgument;

pub enum CyclesError {
    TooFewCycles {},
}

pub trait EstimateRequestCyclesCost {
    fn cycles_cost(&self, request: &CanisterHttpRequestArgument) -> u128;
}

pub struct DefaultRequestCost {
    num_nodes_in_subnet: u32,
}

impl DefaultRequestCost {
    pub const DEFAULT_MAX_RESPONSE_BYTES: u64 = 2_000_000;

    pub fn new(num_nodes_in_subnet: u32) -> Self {
        DefaultRequestCost {
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

impl EstimateRequestCyclesCost for DefaultRequestCost {
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

pub enum CyclesChargingStrategy {
    PaidByCaller,
    PaidByCanister,
}

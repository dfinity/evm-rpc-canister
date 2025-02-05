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
    pub fn new(num_nodes_in_subnet: u32) -> Self {
        DefaultRequestCost {
            num_nodes_in_subnet,
        }
    }
}

impl EstimateRequestCyclesCost for DefaultRequestCost {
    fn cycles_cost(&self, request: &CanisterHttpRequestArgument) -> u128 {
        todo!()
    }
}

pub enum CyclesChargingStrategy {
    PaidByCaller,
    PaidByCanister,
}

use ic_cdk::api::management_canister::http_request::CanisterHttpRequestArgument;

pub enum CyclesError {
    TooFewCycles {},
}

pub trait EstimateRequestCyclesCost {
    fn cycles_cost(&self, request: &CanisterHttpRequestArgument) -> u128;
}

pub struct DefaultRequestCost {}

impl EstimateRequestCyclesCost for DefaultRequestCost {
    fn cycles_cost(&self, request: &CanisterHttpRequestArgument) -> u128 {
        todo!()
    }
}

pub enum CyclesChargingStrategy {
    PaidByCaller,
    PaidByCanister,
}

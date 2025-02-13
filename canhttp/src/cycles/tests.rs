use crate::{CyclesCostEstimator, CyclesChargingPolicy};
use ic_cdk::api::management_canister::http_request::CanisterHttpRequestArgument;

#[test]
fn test_http_request_fee_components() {
    // Assert the calculation matches the cost table at
    // https://internetcomputer.org/docs/current/developer-docs/gas-cost#cycles-price-breakdown
    let estimator = CyclesCostEstimator::new(13);
    assert_eq!(estimator.base_fee(), 49_140_000);
    assert_eq!(estimator.request_fee(1), 5_200);
    assert_eq!(estimator.response_fee(1), 10_400);

    let estimator = CyclesCostEstimator::new(34);
    assert_eq!(estimator.base_fee(), 171_360_000);
    assert_eq!(estimator.request_fee(1), 13_600);
    assert_eq!(estimator.response_fee(1), 27_200);
}

#[test]
fn test_candid_rpc_cost() {
    const OVERHEAD_BYTES: u32 = 356;

    let estimator = CyclesCostEstimator::new(13);
    assert_eq!(
        [
            estimator.cycles_to_attach(&request(0, OVERHEAD_BYTES, 0)),
            estimator.cycles_to_attach(&request(123, OVERHEAD_BYTES, 123)),
            estimator.cycles_to_attach(&request(123, OVERHEAD_BYTES, 4567890)),
            estimator.cycles_to_attach(&request(890, OVERHEAD_BYTES, 4567890)),
        ],
        [50991200, 52910000, 47557686800, 47561675200]
    );

    let estimator = CyclesCostEstimator::new(34);
    assert_eq!(
        [
            estimator.cycles_to_attach(&request(0, OVERHEAD_BYTES, 0)),
            estimator.cycles_to_attach(&request(123, OVERHEAD_BYTES, 123)),
            estimator.cycles_to_attach(&request(123, OVERHEAD_BYTES, 4567890)),
            estimator.cycles_to_attach(&request(890, OVERHEAD_BYTES, 4567890)),
        ],
        [176201600, 181220000, 124424482400, 124434913600]
    );
}

fn request(
    payload_body_bytes: u32,
    extra_payload_bytes: u32,
    max_response_bytes: u64,
) -> CanisterHttpRequestArgument {
    let body = Some(vec![42_u8; payload_body_bytes as usize]);
    let max_response_bytes = Some(max_response_bytes);
    CanisterHttpRequestArgument {
        url: "a".repeat(extra_payload_bytes as usize),
        max_response_bytes,
        method: Default::default(),
        headers: vec![],
        body,
        transform: None,
    }
}

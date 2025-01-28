use crate::constants::{COLLATERAL_CYCLES_PER_NODE, INGRESS_OVERHEAD_BYTES, RPC_URL_COST_BYTES};

/// Calculates the cost of sending a JSON-RPC request using HTTP outcalls.
/// See https://internetcomputer.org/docs/current/developer-docs/gas-cost/#https-outcalls
pub fn get_http_request_cost(
    nodes_in_subnet: u32,
    payload_size_bytes: u64,
    max_response_bytes: u64,
) -> u128 {
    let n = nodes_in_subnet as u128;
    let request_bytes =
        payload_size_bytes as u128 + RPC_URL_COST_BYTES as u128 + INGRESS_OVERHEAD_BYTES;
    base_fee(n) + request_fee(n, request_bytes) + response_fee(n, max_response_bytes as u128)
}

fn base_fee(nodes: u128) -> u128 {
    (3_000_000 + 60_000 * nodes) * nodes
}

fn request_fee(nodes: u128, bytes: u128) -> u128 {
    400 * nodes * bytes
}

fn response_fee(nodes: u128, bytes: u128) -> u128 {
    800 * nodes * bytes
}

/// Calculate the cost + collateral cycles for an HTTP request.
pub fn get_cost_with_collateral(nodes_in_subnet: u32, cycles_cost: u128) -> u128 {
    cycles_cost + COLLATERAL_CYCLES_PER_NODE * nodes_in_subnet as u128
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_request_cost() {
        let nodes_in_subnet = 34;
        let payload = "{\"jsonrpc\":\"2.0\",\"method\":\"eth_gasPrice\",\"params\":[],\"id\":1}";
        let base_cost = get_http_request_cost(nodes_in_subnet, payload.len() as u64, 1000);
        let base_cost_10_extra_bytes =
            get_http_request_cost(nodes_in_subnet, payload.len() as u64 + 10, 1000);
        let estimated_cost_10_extra_bytes = base_cost + 400 * nodes_in_subnet as u128 * 10;
        assert_eq!(base_cost_10_extra_bytes, estimated_cost_10_extra_bytes);
    }

    #[test]
    fn test_http_request_fee_components() {
        // Assert the calculation matches the cost table at
        // https://internetcomputer.org/docs/current/developer-docs/gas-cost#cycles-price-breakdown
        assert_eq!(base_fee(13), 49_140_000);
        assert_eq!(request_fee(13, 1), 5_200);
        assert_eq!(response_fee(13, 1), 10_400);

        assert_eq!(base_fee(34), 171_360_000);
        assert_eq!(request_fee(34, 1), 13_600);
        assert_eq!(response_fee(34, 1), 27_200);
    }

    #[test]
    fn test_candid_rpc_cost() {
        let nodes = 13;
        assert_eq!(
            [
                get_http_request_cost(nodes, 0, 0),
                get_http_request_cost(nodes, 123, 123),
                get_http_request_cost(nodes, 123, 4567890),
                get_http_request_cost(nodes, 890, 4567890),
            ],
            [50991200, 52910000, 47557686800, 47561675200]
        );
        let nodes = 34;
        assert_eq!(
            [
                get_http_request_cost(nodes, 0, 0),
                get_http_request_cost(nodes, 123, 123),
                get_http_request_cost(nodes, 123, 4567890),
                get_http_request_cost(nodes, 890, 4567890),
            ],
            [176201600, 181220000, 124424482400, 124434913600]
        );
    }
}

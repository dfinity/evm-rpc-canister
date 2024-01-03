use cketh_common::eth_rpc_client::providers::RpcApi;

use crate::*;

/// Returns the cycles cost of a JSON-RPC request.
pub fn get_json_rpc_cost(
    source: &ResolvedJsonRpcSource,
    json_rpc_payload: &str,
    max_response_bytes: u64,
) -> u128 {
    match source {
        ResolvedJsonRpcSource::Api(api) => {
            get_http_request_cost(api, json_rpc_payload, max_response_bytes)
        }
        ResolvedJsonRpcSource::Provider(p) => {
            get_http_request_cost(&p.api(), json_rpc_payload, max_response_bytes)
                + get_provider_cost(p, json_rpc_payload.len())
        }
    }
}

/// Returns the cycles cost of a Candid-RPC request.
pub fn get_candid_rpc_cost(
    provider: &Provider,
    payload_size_bytes: usize,
    effective_response_size_estimate: u64,
) -> u128 {
    let base_cost = 400_000_000u128 + 100_000u128 * (2 * effective_response_size_estimate as u128);
    let subnet_size = METADATA.with(|m| m.borrow().get().nodes_in_subnet) as u128;
    let http_cost = base_cost * subnet_size / DEFAULT_NODES_IN_SUBNET as u128;
    let provider_cost = get_provider_cost(provider, payload_size_bytes);
    http_cost + provider_cost
}

/// Calculates the baseline cost of sending a JSON-RPC request using HTTP outcalls.
pub fn get_http_request_cost(
    api: &RpcApi,
    json_rpc_payload: &str,
    max_response_bytes: u64,
) -> u128 {
    let nodes_in_subnet = METADATA.with(|m| m.borrow().get().nodes_in_subnet);
    let ingress_bytes = (json_rpc_payload.len() + api.url.len()) as u128 + INGRESS_OVERHEAD_BYTES;
    let base_cost = INGRESS_MESSAGE_RECEIVED_COST
        + INGRESS_MESSAGE_BYTE_RECEIVED_COST * ingress_bytes
        + HTTP_OUTCALL_REQUEST_COST
        + HTTP_OUTCALL_BYTE_RECEIEVED_COST * (ingress_bytes + max_response_bytes as u128);
    base_cost * (nodes_in_subnet as u128) / DEFAULT_NODES_IN_SUBNET as u128
}

/// Calculates the additional cost for calling a registered JSON-RPC provider.
pub fn get_provider_cost(provider: &Provider, payload_size_bytes: usize) -> u128 {
    let nodes_in_subnet = METADATA.with(|m| m.borrow().get().nodes_in_subnet);
    let cost_per_node = provider.cycles_per_call as u128
        + provider.cycles_per_message_byte as u128 * payload_size_bytes as u128;
    cost_per_node * (nodes_in_subnet as u128)
}

#[test]
fn test_request_cost() {
    METADATA.with(|m| {
        let mut metadata = m.borrow().get().clone();
        metadata.nodes_in_subnet = 13;
        m.borrow_mut().set(metadata).unwrap();
    });

    let url = "https://cloudflare-eth.com";
    let payload = "{\"jsonrpc\":\"2.0\",\"method\":\"eth_gasPrice\",\"params\":[],\"id\":1}";
    let base_cost = get_json_rpc_cost(
        &ResolvedJsonRpcSource::Api(RpcApi {
            url: url.to_string(),
            headers: vec![],
        }),
        payload,
        1000,
    );
    let s10 = "0123456789";
    let base_cost_s10 = get_json_rpc_cost(
        &ResolvedJsonRpcSource::Api(RpcApi {
            url: url.to_string(),
            headers: vec![],
        }),
        &(payload.to_string() + s10),
        1000,
    );
    assert_eq!(
        base_cost + 10 * (INGRESS_MESSAGE_BYTE_RECEIVED_COST + HTTP_OUTCALL_BYTE_RECEIEVED_COST),
        base_cost_s10
    )
}

#[test]
fn test_provider_cost() {
    METADATA.with(|m| {
        let mut metadata = m.borrow().get().clone();
        metadata.nodes_in_subnet = 13;
        m.borrow_mut().set(metadata).unwrap();
    });

    let provider = Provider {
        provider_id: 0,
        hostname: "".to_string(),
        credential_path: "".to_string(),
        credential_headers: vec![],
        owner: Principal::anonymous(),
        chain_id: 1,
        cycles_owed: 0,
        cycles_per_call: 0,
        cycles_per_message_byte: 2,
        primary: false,
    };
    let base_cost = get_provider_cost(
        &provider,
        "{\"jsonrpc\":\"2.0\",\"method\":\"eth_gasPrice\",\"params\":[],\"id\":1}".len(),
    );

    let provider_s10 = Provider {
        provider_id: 0,
        hostname: "".to_string(),
        credential_path: "".to_string(),
        credential_headers: vec![],
        owner: Principal::anonymous(),
        chain_id: 1,
        cycles_owed: 0,
        cycles_per_call: 1000,
        cycles_per_message_byte: 2,
        primary: false,
    };
    let s10 = "0123456789";
    let base_cost_s10 = get_provider_cost(
        &provider_s10,
        "{\"jsonrpc\":\"2.0\",\"method\":\"eth_gasPrice\",\"params\":[],\"id\":1}".len()
            + s10.len(),
    );
    assert_eq!(base_cost + (10 * 2 + 1000) * 13, base_cost_s10)
}

#[test]
fn test_candid_rpc_cost() {
    let provider_id = do_register_provider(
        Principal::anonymous(),
        RegisterProviderArgs {
            chain_id: 0,
            hostname: "rpc.example.com".to_string(),
            credential_headers: None,
            credential_path: "".to_string(),
            cycles_per_call: 999,
            cycles_per_message_byte: 1000,
        },
    );
    let provider = PROVIDERS.with(|providers| providers.borrow().get(&provider_id).unwrap());

    assert_eq!(get_candid_rpc_cost(&provider, 0, 0), 123);
    assert_eq!(get_candid_rpc_cost(&provider, 123, 123), 123);
    assert_eq!(get_candid_rpc_cost(&provider, 123, 4567890), 123);
    assert_eq!(get_candid_rpc_cost(&provider, 890, 4567890), 123);
}

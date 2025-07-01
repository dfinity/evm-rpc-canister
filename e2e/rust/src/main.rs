use std::str::FromStr;

use candid::{candid_method, Principal};
use ic_cdk_macros::update;

use evm_rpc_types::{
    Block, BlockTag, ConsensusStrategy, EthMainnetService, Hex32, MultiRpcResult, ProviderError,
    RpcConfig, RpcError, RpcResult, RpcService, RpcServices,
};

fn main() {}

#[cfg(target_arch = "wasm32")]
const CANISTER_ID: Option<&str> = Some(std::env!(
    "CANISTER_ID_EVM_RPC_STAGING",
    "Unspecified canister ID environment variable"
));
#[cfg(not(target_arch = "wasm32"))]
const CANISTER_ID: Option<&str> = None;

#[update]
#[candid_method(update)]
pub async fn test() {
    assert!(ic_cdk::api::is_controller(&ic_cdk::caller()));

    let canister_id = Principal::from_str(CANISTER_ID.unwrap())
        .expect("Error parsing canister ID environment variable");

    // Define request parameters
    let params = (
        RpcService::EthMainnet(EthMainnetService::PublicNode), // Ethereum mainnet
        "{\"jsonrpc\":\"2.0\",\"method\":\"eth_gasPrice\",\"params\":null,\"id\":1}".to_string(),
        1000 as u64,
    );

    // Get cycles cost
    let (cycles_result,): (Result<u128, RpcError>,) =
        ic_cdk::api::call::call(canister_id, "requestCost", params.clone())
            .await
            .unwrap();
    let cycles = cycles_result
        .unwrap_or_else(|e| ic_cdk::trap(&format!("error in `request_cost`: {:?}", e)));

    // Call without sending cycles
    let (result_without_cycles,): (Result<String, RpcError>,) =
        ic_cdk::api::call::call(canister_id, "request", params.clone())
            .await
            .unwrap();
    match result_without_cycles {
        Ok(s) => ic_cdk::trap(&format!("response from `request` without cycles: {:?}", s)),
        Err(RpcError::ProviderError(ProviderError::TooFewCycles { expected, .. })) => {
            assert_eq!(expected, cycles)
        }
        Err(err) => ic_cdk::trap(&format!("error in `request` without cycles: {:?}", err)),
    }

    // Call with expected number of cycles
    let (result,): (Result<String, RpcError>,) =
        ic_cdk::api::call::call_with_payment128(canister_id, "request", params, cycles)
            .await
            .unwrap();
    match result {
        Ok(response) => {
            // Check response structure around gas price
            assert_eq!(
                &response[..36],
                "{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":\"0x"
            );
            assert_eq!(&response[response.len() - 2..], "\"}");
        }
        Err(err) => ic_cdk::trap(&format!("error in `request` with cycles: {:?}", err)),
    }

    // Call a Candid-RPC method
    let (results,): (MultiRpcResult<Block>,) = ic_cdk::api::call::call_with_payment128(
        canister_id,
        "eth_getBlockByNumber",
        (
            RpcServices::EthMainnet(Some(vec![
                // EthMainnetService::Ankr, // Need API key
                EthMainnetService::BlockPi,
                EthMainnetService::Llama,
                EthMainnetService::PublicNode,
            ])),
            Some(RpcConfig {
                response_consensus: Some(ConsensusStrategy::Threshold {
                    total: None,
                    min: 2,
                }),
                ..Default::default()
            }),
            BlockTag::Number(19709434_u32.into()),
        ),
        10000000000,
    )
    .await
    .unwrap();
    match results {
        MultiRpcResult::Consistent(result) => match result {
            Ok(block) => {
                assert_eq!(
                    block.hash,
                    Hex32::from_str(
                        "0x114755458f57fe1a81e7b03e038ad00f9a675681c8b94cf102c30a84c5545c76"
                    )
                    .unwrap()
                );
            }
            Err(err) => ic_cdk::trap(&format!("error in `eth_getBlockByNumber`: {:?}", err)),
        },
        MultiRpcResult::Inconsistent(results) => ic_cdk::trap(&format!(
            "inconsistent results in `eth_getBlockByNumber`: {}",
            debug_inconsistent(&results)
        )),
    }
}

fn debug_inconsistent(results: &[(RpcService, RpcResult<Block>)]) -> String {
    results
        .iter()
        .map(|(service, result)| {
            let res_str = match result {
                // A block contains too much information and in case of inconsistencies
                // not all results can be displayed, so we only display the block number
                // assuming that responses with the same block number should be equal.
                Ok(block) => format!("Ok({}, ...)", block.number),
                Err(err) => format!("Err({err})"),
            };
            format!("{service:?}: {res_str}")
        })
        .collect::<Vec<_>>()
        .join(", ")
}

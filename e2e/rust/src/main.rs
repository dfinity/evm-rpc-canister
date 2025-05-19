use std::str::FromStr;

use candid::Principal;
use ic_cdk::call::Call;
use ic_cdk::update;

use evm_rpc_types::{
    Block, BlockTag, EthMainnetService, Hex32, MultiRpcResult, ProviderError, RpcError, RpcService,
    RpcServices,
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
pub async fn test() {
    assert!(ic_cdk::api::is_controller(&ic_cdk::api::msg_caller()));

    let canister_id = Principal::from_str(CANISTER_ID.unwrap())
        .expect("Error parsing canister ID environment variable");

    // Define request parameters
    let params = (
        RpcService::EthMainnet(EthMainnetService::PublicNode), // Ethereum mainnet
        "{\"jsonrpc\":\"2.0\",\"method\":\"eth_gasPrice\",\"params\":null,\"id\":1}".to_string(),
        1000u64,
    );

    // Get cycles cost
    let cycles_result: Result<u128, RpcError> = Call::unbounded_wait(canister_id, "requestCost")
        .with_args(&params)
        .await
        .unwrap()
        .candid()
        .unwrap();
    let cycles =
        cycles_result.unwrap_or_else(|e| ic_cdk::trap(format!("error in `request_cost`: {:?}", e)));

    // Call without sending cycles
    let result_without_cycles: Result<String, RpcError> =
        Call::unbounded_wait(canister_id, "request")
            .with_args(&params)
            .await
            .unwrap()
            .candid()
            .unwrap();
    match result_without_cycles {
        Ok(s) => ic_cdk::trap(format!("response from `request` without cycles: {:?}", s)),
        Err(RpcError::ProviderError(ProviderError::TooFewCycles { expected, .. })) => {
            assert_eq!(expected, cycles)
        }
        Err(err) => ic_cdk::trap(format!("error in `request` without cycles: {:?}", err)),
    }

    // Call with expected number of cycles
    let result: Result<String, RpcError> = Call::unbounded_wait(canister_id, "request")
        .with_args(&params)
        .with_cycles(cycles)
        .await
        .unwrap()
        .candid()
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
        Err(err) => ic_cdk::trap(format!("error in `request` with cycles: {:?}", err)),
    }

    // Call a Candid-RPC method
    let results: MultiRpcResult<Block> = Call::unbounded_wait(canister_id, "eth_getBlockByNumber")
        .with_args(&(
            RpcServices::EthMainnet(Some(vec![
                // EthMainnetService::Ankr, // Need API key
                EthMainnetService::BlockPi,
                EthMainnetService::Llama,
                EthMainnetService::PublicNode,
            ])),
            (),
            BlockTag::Number(19709434_u32.into()),
        ))
        .with_cycles(10000000000)
        .await
        .unwrap()
        .candid()
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
            Err(err) => ic_cdk::trap(format!("error in `eth_getBlockByNumber`: {:?}", err)),
        },
        MultiRpcResult::Inconsistent(results) => ic_cdk::trap(format!(
            "inconsistent results in `eth_getBlockByNumber`: {:?}",
            results
        )),
    }
}

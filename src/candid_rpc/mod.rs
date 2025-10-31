mod cketh_conversion;

use crate::rpc_client::EthRpcClient;
use crate::types::RpcMethod;
use candid::Nat;
use canhttp::http::json::JsonRpcRequest;
use canhttp::multi::Timestamp;
use ethers_core::{types::Transaction, utils::rlp};
use evm_rpc_types::{
    BlockTag, Hex, Hex32, MultiRpcResult, Nat256, RpcError, RpcResult, ValidationError,
};

/// Adapt the `EthRpcClient` to the `Candid` interface used by the EVM-RPC canister.
pub struct CandidRpcClient {
    client: EthRpcClient,
}

impl CandidRpcClient {
    pub fn new(
        source: evm_rpc_types::RpcServices,
        config: Option<evm_rpc_types::RpcConfig>,
        now: Timestamp,
    ) -> RpcResult<Self> {
        Ok(Self {
            client: EthRpcClient::new(source, config, now)?,
        })
    }

    pub async fn eth_get_logs(
        self,
        args: evm_rpc_types::GetLogsArgs,
    ) -> MultiRpcResult<Vec<evm_rpc_types::LogEntry>> {
        use crate::candid_rpc::cketh_conversion::{from_log_entries, into_get_logs_param};
        self.client
            .eth_get_logs(into_get_logs_param(args))
            .send_and_reduce()
            .await
            .map(from_log_entries)
    }

    pub async fn eth_get_block_by_number(
        self,
        block: BlockTag,
    ) -> MultiRpcResult<evm_rpc_types::Block> {
        use crate::candid_rpc::cketh_conversion::{from_block, into_block_spec};
        self.client
            .eth_get_block_by_number(into_block_spec(block))
            .send_and_reduce()
            .await
            .map(from_block)
    }

    pub async fn eth_get_transaction_receipt(
        self,
        hash: Hex32,
    ) -> MultiRpcResult<Option<evm_rpc_types::TransactionReceipt>> {
        use crate::candid_rpc::cketh_conversion::{from_transaction_receipt, into_hash};
        self.client
            .eth_get_transaction_receipt(into_hash(hash))
            .send_and_reduce()
            .await
            .map(|option| option.map(from_transaction_receipt))
    }

    pub async fn eth_get_transaction_count(
        self,
        args: evm_rpc_types::GetTransactionCountArgs,
    ) -> MultiRpcResult<Nat256> {
        use crate::candid_rpc::cketh_conversion::into_get_transaction_count_params;
        self.client
            .eth_get_transaction_count(into_get_transaction_count_params(args))
            .send_and_reduce()
            .await
            .map(Nat256::from)
    }

    pub async fn eth_fee_history(
        self,
        args: evm_rpc_types::FeeHistoryArgs,
    ) -> MultiRpcResult<evm_rpc_types::FeeHistory> {
        use crate::candid_rpc::cketh_conversion::{from_fee_history, into_fee_history_params};
        self.client
            .eth_fee_history(into_fee_history_params(args))
            .send_and_reduce()
            .await
            .map(from_fee_history)
    }

    pub async fn eth_send_raw_transaction(
        self,
        raw_signed_transaction_hex: Hex,
    ) -> MultiRpcResult<evm_rpc_types::SendRawTransactionStatus> {
        use crate::candid_rpc::cketh_conversion::from_send_raw_transaction_result;
        let transaction_hash = get_transaction_hash(&raw_signed_transaction_hex);
        self.client
            .eth_send_raw_transaction(raw_signed_transaction_hex.to_string())
            .send_and_reduce()
            .await
            .map(|result| from_send_raw_transaction_result(transaction_hash.clone(), result))
    }

    pub async fn eth_call(self, args: evm_rpc_types::CallArgs) -> MultiRpcResult<Hex> {
        use crate::candid_rpc::cketh_conversion::{from_data, into_eth_call_params};
        self.client
            .eth_call(into_eth_call_params(args))
            .send_and_reduce()
            .await
            .map(from_data)
    }

    pub async fn multi_request(self, json_rpc_payload: String) -> MultiRpcResult<String> {
        let request: JsonRpcRequest<serde_json::Value> =
            match serde_json::from_str(&json_rpc_payload) {
                Ok(req) => req,
                Err(e) => {
                    return MultiRpcResult::Consistent(Err(RpcError::ValidationError(
                        ValidationError::Custom(format!("Invalid JSON RPC request: {e}")),
                    )))
                }
            };
        self.client
            .multi_request(
                RpcMethod::Custom(request.method().to_string()),
                request.params(),
            )
            .send_and_reduce()
            .await
            .map(String::from)
    }
}

fn get_transaction_hash(raw_signed_transaction_hex: &Hex) -> Option<Hex32> {
    let transaction: Transaction = rlp::decode(raw_signed_transaction_hex.as_ref()).ok()?;
    Some(Hex32::from(transaction.hash.0))
}

pub fn validate_get_logs_block_range(
    args: &evm_rpc_types::GetLogsArgs,
    max_block_range: u32,
) -> RpcResult<()> {
    if let (Some(BlockTag::Number(from)), Some(BlockTag::Number(to))) =
        (&args.from_block, &args.to_block)
    {
        let from = Nat::from(from.clone());
        let to = Nat::from(to.clone());
        let block_count = if to > from { to - from } else { from - to };
        if block_count > max_block_range {
            return Err(ValidationError::Custom(format!(
                "Requested {} blocks; limited to {} when specifying a start and end block",
                block_count, max_block_range
            ))
            .into());
        }
    }
    Ok(())
}

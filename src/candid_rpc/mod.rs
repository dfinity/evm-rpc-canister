mod cketh_conversion;

use crate::{
    candid_rpc::cketh_conversion::into_json_request, rpc_client::EthRpcClient, types::RpcMethod,
};
use candid::Nat;
use canhttp::multi::Timestamp;
use ethers_core::{types::Transaction, utils::rlp};
use evm_rpc_types::{BlockTag, Hex, Hex32, MultiRpcResult, Nat256, RpcResult, ValidationError};

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
        use cketh_conversion::{from_log_entries, into_get_logs_param};
        self.client
            .eth_get_logs(into_get_logs_param(args))
            .send_and_reduce()
            .await
            .map(from_log_entries)
    }

    pub async fn eth_get_logs_cycles_cost(
        self,
        args: evm_rpc_types::GetLogsArgs,
    ) -> RpcResult<u128> {
        use cketh_conversion::into_get_logs_param;
        self.client
            .eth_get_logs(into_get_logs_param(args))
            .cycles_cost()
            .await
    }

    pub async fn eth_get_block_by_number(
        self,
        block: BlockTag,
    ) -> MultiRpcResult<evm_rpc_types::Block> {
        use cketh_conversion::{from_block, into_block_spec};
        self.client
            .eth_get_block_by_number(into_block_spec(block))
            .send_and_reduce()
            .await
            .map(from_block)
    }

    pub async fn eth_get_block_by_number_cycles_cost(self, block: BlockTag) -> RpcResult<u128> {
        use cketh_conversion::into_block_spec;
        self.client
            .eth_get_block_by_number(into_block_spec(block))
            .cycles_cost()
            .await
    }

    pub async fn eth_get_transaction_receipt(
        self,
        hash: Hex32,
    ) -> MultiRpcResult<Option<evm_rpc_types::TransactionReceipt>> {
        use cketh_conversion::{from_transaction_receipt, into_hash};
        self.client
            .eth_get_transaction_receipt(into_hash(hash))
            .send_and_reduce()
            .await
            .map(|option| option.map(from_transaction_receipt))
    }

    pub async fn eth_get_transaction_receipt_cycles_cost(self, hash: Hex32) -> RpcResult<u128> {
        use cketh_conversion::into_hash;
        self.client
            .eth_get_transaction_receipt(into_hash(hash))
            .cycles_cost()
            .await
    }

    pub async fn eth_get_transaction_count(
        self,
        args: evm_rpc_types::GetTransactionCountArgs,
    ) -> MultiRpcResult<Nat256> {
        use cketh_conversion::into_get_transaction_count_params;
        self.client
            .eth_get_transaction_count(into_get_transaction_count_params(args))
            .send_and_reduce()
            .await
            .map(Nat256::from)
    }

    pub async fn eth_get_transaction_count_cycles_cost(
        self,
        args: evm_rpc_types::GetTransactionCountArgs,
    ) -> RpcResult<u128> {
        use cketh_conversion::into_get_transaction_count_params;
        self.client
            .eth_get_transaction_count(into_get_transaction_count_params(args))
            .cycles_cost()
            .await
    }

    pub async fn eth_fee_history(
        self,
        args: evm_rpc_types::FeeHistoryArgs,
    ) -> MultiRpcResult<evm_rpc_types::FeeHistory> {
        use cketh_conversion::{from_fee_history, into_fee_history_params};
        self.client
            .eth_fee_history(into_fee_history_params(args))
            .send_and_reduce()
            .await
            .map(from_fee_history)
    }

    pub async fn eth_fee_history_cycles_cost(
        self,
        args: evm_rpc_types::FeeHistoryArgs,
    ) -> RpcResult<u128> {
        use cketh_conversion::into_fee_history_params;
        self.client
            .eth_fee_history(into_fee_history_params(args))
            .cycles_cost()
            .await
    }

    pub async fn eth_send_raw_transaction(
        self,
        raw_signed_transaction_hex: Hex,
    ) -> MultiRpcResult<evm_rpc_types::SendRawTransactionStatus> {
        use cketh_conversion::from_send_raw_transaction_result;
        let transaction_hash = get_transaction_hash(&raw_signed_transaction_hex);
        self.client
            .eth_send_raw_transaction(raw_signed_transaction_hex.to_string())
            .send_and_reduce()
            .await
            .map(|result| from_send_raw_transaction_result(transaction_hash.clone(), result))
    }

    pub async fn eth_send_raw_transaction_cycles_cost(
        self,
        raw_signed_transaction_hex: Hex,
    ) -> RpcResult<u128> {
        self.client
            .eth_send_raw_transaction(raw_signed_transaction_hex.to_string())
            .cycles_cost()
            .await
    }

    pub async fn eth_call(self, args: evm_rpc_types::CallArgs) -> MultiRpcResult<Hex> {
        use cketh_conversion::{from_data, into_eth_call_params};
        self.client
            .eth_call(into_eth_call_params(args))
            .send_and_reduce()
            .await
            .map(from_data)
    }

    pub async fn eth_call_cycles_cost(self, args: evm_rpc_types::CallArgs) -> RpcResult<u128> {
        use cketh_conversion::into_eth_call_params;
        self.client
            .eth_call(into_eth_call_params(args))
            .cycles_cost()
            .await
    }

    pub async fn multi_request(self, json_rpc_payload: String) -> MultiRpcResult<String> {
        let request = match into_json_request(json_rpc_payload) {
            Ok(request) => request,
            Err(err) => return MultiRpcResult::Consistent(Err(err)),
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

    pub async fn multi_cycles_cost(self, json_rpc_payload: String) -> RpcResult<u128> {
        let request = into_json_request(json_rpc_payload)?;
        self.client
            .multi_request(
                RpcMethod::Custom(request.method().to_string()),
                request.params(),
            )
            .cycles_cost()
            .await
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

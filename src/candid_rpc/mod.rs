mod cketh_conversion;
#[cfg(test)]
mod tests;

use crate::rpc_client::{EthRpcClient, ReducedResult};
use crate::{
    add_metric_entry,
    constants::ETH_GET_LOGS_MAX_BLOCKS,
    providers::resolve_rpc_service,
    types::{MetricRpcHost, ResolvedRpcService, RpcMethod},
};
use candid::Nat;
use canhttp::ReductionError;
use ethers_core::{types::Transaction, utils::rlp};
use evm_rpc_types::{Hex, Hex32, MultiRpcResult, Nat256, RpcResult, ValidationError};

fn process_result<T>(method: RpcMethod, result: ReducedResult<T>) -> MultiRpcResult<T> {
    match result {
        Ok(value) => MultiRpcResult::Consistent(Ok(value)),
        Err(err) => match err {
            ReductionError::ConsistentError(err) => MultiRpcResult::Consistent(Err(err)),
            ReductionError::InconsistentResults(multi_call_results) => {
                let results = multi_call_results.into_vec();
                results.iter().for_each(|(service, _service_result)| {
                    if let Ok(ResolvedRpcService::Provider(provider)) =
                        resolve_rpc_service(service.clone())
                    {
                        add_metric_entry!(
                            inconsistent_responses,
                            (
                                method.into(),
                                MetricRpcHost(
                                    provider
                                        .hostname()
                                        .unwrap_or_else(|| "(unknown)".to_string())
                                )
                            ),
                            1
                        )
                    }
                });
                MultiRpcResult::Inconsistent(results)
            }
        },
    }
}

/// Adapt the `EthRpcClient` to the `Candid` interface used by the EVM-RPC canister.
pub struct CandidRpcClient {
    client: EthRpcClient,
}

impl CandidRpcClient {
    pub fn new(
        source: evm_rpc_types::RpcServices,
        config: Option<evm_rpc_types::RpcConfig>,
    ) -> RpcResult<Self> {
        Ok(Self {
            client: EthRpcClient::new(source, config)?,
        })
    }

    pub async fn eth_get_logs(
        &self,
        args: evm_rpc_types::GetLogsArgs,
    ) -> MultiRpcResult<Vec<evm_rpc_types::LogEntry>> {
        use crate::candid_rpc::cketh_conversion::{from_log_entries, into_get_logs_param};

        if let (
            Some(evm_rpc_types::BlockTag::Number(from)),
            Some(evm_rpc_types::BlockTag::Number(to)),
        ) = (&args.from_block, &args.to_block)
        {
            let from = Nat::from(from.clone());
            let to = Nat::from(to.clone());
            let block_count = if to > from { to - from } else { from - to };
            if block_count > ETH_GET_LOGS_MAX_BLOCKS {
                return MultiRpcResult::Consistent(Err(ValidationError::Custom(format!(
                    "Requested {} blocks; limited to {} when specifying a start and end block",
                    block_count, ETH_GET_LOGS_MAX_BLOCKS
                ))
                .into()));
            }
        }
        process_result(
            RpcMethod::EthGetLogs,
            self.client.eth_get_logs(into_get_logs_param(args)).await,
        )
        .map(from_log_entries)
    }

    pub async fn eth_get_block_by_number(
        &self,
        block: evm_rpc_types::BlockTag,
    ) -> MultiRpcResult<evm_rpc_types::Block> {
        use crate::candid_rpc::cketh_conversion::{from_block, into_block_spec};
        process_result(
            RpcMethod::EthGetBlockByNumber,
            self.client
                .eth_get_block_by_number(into_block_spec(block))
                .await,
        )
        .map(from_block)
    }

    pub async fn eth_get_transaction_receipt(
        &self,
        hash: Hex32,
    ) -> MultiRpcResult<Option<evm_rpc_types::TransactionReceipt>> {
        use crate::candid_rpc::cketh_conversion::{from_transaction_receipt, into_hash};
        process_result(
            RpcMethod::EthGetTransactionReceipt,
            self.client
                .eth_get_transaction_receipt(into_hash(hash))
                .await,
        )
        .map(|option| option.map(from_transaction_receipt))
    }

    pub async fn eth_get_transaction_count(
        &self,
        args: evm_rpc_types::GetTransactionCountArgs,
    ) -> MultiRpcResult<Nat256> {
        use crate::candid_rpc::cketh_conversion::into_get_transaction_count_params;
        process_result(
            RpcMethod::EthGetTransactionCount,
            self.client
                .eth_get_transaction_count(into_get_transaction_count_params(args))
                .await,
        )
        .map(Nat256::from)
    }

    pub async fn eth_fee_history(
        &self,
        args: evm_rpc_types::FeeHistoryArgs,
    ) -> MultiRpcResult<evm_rpc_types::FeeHistory> {
        use crate::candid_rpc::cketh_conversion::{from_fee_history, into_fee_history_params};
        process_result(
            RpcMethod::EthFeeHistory,
            self.client
                .eth_fee_history(into_fee_history_params(args))
                .await,
        )
        .map(from_fee_history)
    }

    pub async fn eth_send_raw_transaction(
        &self,
        raw_signed_transaction_hex: Hex,
    ) -> MultiRpcResult<evm_rpc_types::SendRawTransactionStatus> {
        use crate::candid_rpc::cketh_conversion::from_send_raw_transaction_result;
        let transaction_hash = get_transaction_hash(&raw_signed_transaction_hex);
        process_result(
            RpcMethod::EthSendRawTransaction,
            self.client
                .eth_send_raw_transaction(raw_signed_transaction_hex.to_string())
                .await,
        )
        .map(|result| from_send_raw_transaction_result(transaction_hash.clone(), result))
    }

    pub async fn eth_call(
        &self,
        args: evm_rpc_types::CallArgs,
    ) -> MultiRpcResult<evm_rpc_types::Hex> {
        use crate::candid_rpc::cketh_conversion::{from_data, into_eth_call_params};
        process_result(
            RpcMethod::EthFeeHistory,
            self.client.eth_call(into_eth_call_params(args)).await,
        )
        .map(from_data)
    }
}

fn get_transaction_hash(raw_signed_transaction_hex: &Hex) -> Option<Hex32> {
    let transaction: Transaction = rlp::decode(raw_signed_transaction_hex.as_ref()).ok()?;
    Some(Hex32::from(transaction.hash.0))
}

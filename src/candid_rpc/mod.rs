use crate::rpc_client::EthRpcClient;
use candid::Nat;
use canhttp::multi::Timestamp;
use ethers_core::{types::Transaction, utils::rlp};
use evm_rpc_types::{
    BlockTag, EvmRpcRequest, EvmRpcResponse, Hex, Hex32, MultiRpcResult, Nat256, RpcError,
    RpcResult, ValidationError,
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
        request: EvmRpcRequest,
    ) -> MultiRpcResult<Vec<evm_rpc_types::LogEntry>> {
        self.send_request(request)
            .await
            .map(|response| response.try_unwrap_get_logs().unwrap())
    }

    pub async fn eth_get_block_by_number(
        self,
        request: EvmRpcRequest,
    ) -> MultiRpcResult<evm_rpc_types::Block> {
        self.send_request(request)
            .await
            .map(|response| response.try_unwrap_get_block_by_number().unwrap())
    }

    pub async fn eth_get_transaction_receipt(
        self,
        request: EvmRpcRequest,
    ) -> MultiRpcResult<Option<evm_rpc_types::TransactionReceipt>> {
        self.send_request(request)
            .await
            .map(|response| response.try_unwrap_get_transaction_receipt().unwrap())
    }

    pub async fn eth_get_transaction_count(self, request: EvmRpcRequest) -> MultiRpcResult<Nat256> {
        self.send_request(request)
            .await
            .map(|response| response.try_unwrap_get_transaction_count().unwrap())
    }

    pub async fn eth_fee_history(
        self,
        request: EvmRpcRequest,
    ) -> MultiRpcResult<evm_rpc_types::FeeHistory> {
        self.send_request(request)
            .await
            .map(|response| response.try_unwrap_fee_history().unwrap())
    }

    pub async fn eth_send_raw_transaction(
        self,
        request: EvmRpcRequest,
    ) -> MultiRpcResult<evm_rpc_types::SendRawTransactionStatus> {
        let tx_hash = match request.try_unwrap_send_raw_transaction_ref() {
            Ok(transaction) => get_transaction_hash(transaction),
            Err(_) => {
                return consistent_error(RpcError::ValidationError(ValidationError::Custom(
                    "Unable to parse `eth_sendRawTransaction` arguments".to_string(),
                )))
            }
        };

        self.send_request(request)
            .await
            .map(|response| response.try_unwrap_send_raw_transaction().unwrap())
            .map(|status| match status {
                evm_rpc_types::SendRawTransactionStatus::Ok(_) => {
                    evm_rpc_types::SendRawTransactionStatus::Ok(tx_hash.clone())
                }
                status => status,
            })
    }

    pub async fn eth_call(self, request: EvmRpcRequest) -> MultiRpcResult<evm_rpc_types::Hex> {
        self.send_request(request)
            .await
            .map(|response| response.try_unwrap_call().unwrap())
    }

    pub async fn multi_request(self, request: EvmRpcRequest) -> MultiRpcResult<String> {
        self.send_request(request)
            .await
            .map(|response| response.try_unwrap_json_rpc_request().unwrap())
    }

    pub async fn cycles_cost(self, request: EvmRpcRequest) -> RpcResult<u128> {
        self.client
            .multi_rpc_request(request.try_into()?)
            .cycles_cost()
            .await
    }

    async fn send_request(self, request: EvmRpcRequest) -> MultiRpcResult<EvmRpcResponse> {
        let request = match request.try_into() {
            Ok(request) => request,
            Err(e) => return consistent_error(e),
        };
        self.client
            .multi_rpc_request(request)
            .send_and_reduce()
            .await
            .map(EvmRpcResponse::from)
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

fn consistent_error<T>(err: RpcError) -> MultiRpcResult<T> {
    MultiRpcResult::Consistent(Err(err))
}

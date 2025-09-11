use crate::{
    Block, FeeHistory, Hex, JsonRpcError, LogEntry, MultiRpcResult, Nat256, RpcError,
    SendRawTransactionStatus,
};

impl From<MultiRpcResult<Vec<LogEntry>>> for MultiRpcResult<Vec<alloy_rpc_types::Log>> {
    fn from(result: MultiRpcResult<Vec<LogEntry>>) -> Self {
        result.and_then(|logs| {
            logs.into_iter()
                .map(alloy_rpc_types::Log::try_from)
                .collect()
        })
    }
}

impl From<MultiRpcResult<Block>> for MultiRpcResult<alloy_rpc_types::Block> {
    fn from(result: MultiRpcResult<Block>) -> Self {
        result.and_then(alloy_rpc_types::Block::try_from)
    }
}

impl From<MultiRpcResult<FeeHistory>> for MultiRpcResult<alloy_rpc_types::FeeHistory> {
    fn from(result: MultiRpcResult<FeeHistory>) -> Self {
        result.and_then(alloy_rpc_types::FeeHistory::try_from)
    }
}

impl From<MultiRpcResult<Nat256>> for MultiRpcResult<alloy_primitives::U256> {
    fn from(result: MultiRpcResult<Nat256>) -> Self {
        result.map(alloy_primitives::U256::from)
    }
}

impl From<MultiRpcResult<Hex>> for MultiRpcResult<alloy_primitives::Bytes> {
    fn from(result: MultiRpcResult<Hex>) -> Self {
        result.map(alloy_primitives::Bytes::from)
    }
}

impl From<MultiRpcResult<SendRawTransactionStatus>>
    for MultiRpcResult<Option<alloy_primitives::B256>>
{
    fn from(result: MultiRpcResult<SendRawTransactionStatus>) -> Self {
        result.and_then(|status| match status {
            SendRawTransactionStatus::Ok(maybe_hash) => {
                Ok(maybe_hash.map(alloy_primitives::B256::from))
            }
            error => Err(RpcError::JsonRpcError(JsonRpcError {
                code: -32_000,
                message: match error {
                    SendRawTransactionStatus::Ok(_) => unreachable!(),
                    SendRawTransactionStatus::InsufficientFunds => "Insufficient funds",
                    SendRawTransactionStatus::NonceTooLow => "Nonce too low",
                    SendRawTransactionStatus::NonceTooHigh => "Nonce too high",
                }
                .to_string(),
            })),
        })
    }
}

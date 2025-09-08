use crate::{Block, FeeHistory, LogEntry, MultiRpcResult, Nat256};

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

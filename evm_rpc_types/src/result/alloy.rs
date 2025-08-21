use crate::{LogEntry, MultiRpcResult};

impl From<MultiRpcResult<Vec<LogEntry>>> for MultiRpcResult<Vec<alloy_rpc_types::Log>> {
    fn from(result: MultiRpcResult<Vec<LogEntry>>) -> Self {
        result.and_then(|logs| {
            logs.into_iter()
                .map(alloy_rpc_types::Log::try_from)
                .collect()
        })
    }
}

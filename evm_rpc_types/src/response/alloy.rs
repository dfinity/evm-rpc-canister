use crate::{LogEntry, RpcError, ValidationError};

impl TryFrom<LogEntry> for alloy_rpc_types::Log {
    type Error = RpcError;

    fn try_from(entry: LogEntry) -> Result<Self, Self::Error> {
        Ok(Self {
            inner: alloy_primitives::Log {
                address: alloy_primitives::Address::from(entry.address),
                data: alloy_primitives::LogData::new(
                    entry
                        .topics
                        .into_iter()
                        .map(alloy_primitives::B256::from)
                        .collect(),
                    alloy_primitives::Bytes::from(entry.data),
                )
                .ok_or(RpcError::ValidationError(ValidationError::Custom(
                    "Invalid log data".to_string(),
                )))?,
            },
            block_hash: entry.block_hash.map(alloy_primitives::BlockHash::from),
            block_number: entry.block_number.map(u64::try_from).transpose()?,
            block_timestamp: None,
            transaction_hash: entry.transaction_hash.map(alloy_primitives::TxHash::from),
            transaction_index: entry.transaction_index.map(u64::try_from).transpose()?,
            log_index: entry.log_index.map(u64::try_from).transpose()?,
            removed: entry.removed,
        })
    }
}

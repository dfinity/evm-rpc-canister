use crate::{Block, FeeHistory, LogEntry, Nat256, RpcError, ValidationError};
use alloy_rpc_types::BlockTransactions;
use candid::Nat;
use num_bigint::BigUint;
use std::{any::type_name, fmt::Debug};

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
            block_number: entry
                .block_number
                .map(|value| try_from_nat256(value, "block_number"))
                .transpose()?,
            block_timestamp: None,
            transaction_hash: entry.transaction_hash.map(alloy_primitives::TxHash::from),
            transaction_index: entry
                .transaction_index
                .map(|value| try_from_nat256(value, "transaction_index"))
                .transpose()?,
            log_index: entry
                .log_index
                .map(|value| try_from_nat256(value, "log_index"))
                .transpose()?,
            removed: entry.removed,
        })
    }
}

impl TryFrom<Block> for alloy_rpc_types::Block {
    type Error = RpcError;

    fn try_from(value: Block) -> Result<Self, Self::Error> {
        Ok(Self {
            header: alloy_rpc_types::Header {
                hash: alloy_primitives::BlockHash::from(value.hash),
                inner: alloy_consensus::Header {
                    parent_hash: alloy_primitives::BlockHash::from(value.parent_hash),
                    ommers_hash: alloy_primitives::BlockHash::from(value.sha3_uncles),
                    beneficiary: alloy_primitives::Address::from(value.miner),
                    state_root: alloy_primitives::B256::from(value.state_root),
                    transactions_root: alloy_primitives::B256::from(
                        value.transactions_root.ok_or(RpcError::ValidationError(
                            ValidationError::Custom(
                                "Block does not have a transactions root field".to_string(),
                            ),
                        ))?,
                    ),
                    receipts_root: alloy_primitives::B256::from(value.receipts_root),
                    logs_bloom: alloy_primitives::Bloom::from(value.logs_bloom),
                    difficulty: value
                        .difficulty
                        .ok_or(RpcError::ValidationError(ValidationError::Custom(
                            "Block does not have a difficulty field".to_string(),
                        )))?
                        .into(),
                    number: try_from_nat256(value.number, "number")?,
                    gas_limit: try_from_nat256(value.gas_limit, "gas_limit")?,
                    gas_used: try_from_nat256(value.gas_used, "gas_used")?,
                    timestamp: try_from_nat256(value.timestamp, "timestamp")?,
                    extra_data: alloy_primitives::Bytes::from(value.extra_data),
                    mix_hash: alloy_primitives::B256::from(value.mix_hash),
                    nonce: alloy_primitives::B64::try_from(value.nonce)?,
                    base_fee_per_gas: value
                        .base_fee_per_gas
                        .map(|value| try_from_nat256(value, "base_fee_per_gas"))
                        .transpose()?,
                    withdrawals_root: None,
                    blob_gas_used: None,
                    excess_blob_gas: None,
                    parent_beacon_block_root: None,
                    requests_hash: None,
                },
                total_difficulty: value.total_difficulty.map(|value| value.into()),
                size: Some(value.size.into()),
            },
            uncles: value
                .uncles
                .into_iter()
                .map(alloy_primitives::B256::from)
                .collect(),
            transactions: BlockTransactions::Hashes(
                value
                    .transactions
                    .into_iter()
                    .map(alloy_primitives::B256::from)
                    .collect(),
            ),
            withdrawals: None,
        })
    }
}

impl TryFrom<FeeHistory> for alloy_rpc_types::FeeHistory {
    type Error = RpcError;

    fn try_from(value: FeeHistory) -> Result<Self, Self::Error> {
        Ok(Self {
            base_fee_per_gas: value
                .base_fee_per_gas
                .into_iter()
                .map(|reward| try_from_nat256(reward, "base_fee_per_gas"))
                .collect::<Result<Vec<_>, _>>()?,
            gas_used_ratio: value.gas_used_ratio,
            base_fee_per_blob_gas: vec![],
            blob_gas_used_ratio: vec![],
            oldest_block: try_from_nat256(value.oldest_block, "oldest_block")?,
            reward: Some(
                value
                    .reward
                    .into_iter()
                    .map(|rewards| {
                        rewards
                            .into_iter()
                            .map(|reward| try_from_nat256(reward, "reward"))
                            .collect::<Result<Vec<_>, _>>()
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            ),
        })
    }
}

fn try_from_nat256<T: TryFrom<BigUint, Error = E>, E: Debug>(
    value: Nat256,
    field_name: &str,
) -> Result<T, RpcError> {
    T::try_from(Nat::from(value).0).map_err(|err| {
        RpcError::ValidationError(ValidationError::Custom(format!(
            "Failed to convert field `{}` to `{}`: {:?}",
            field_name,
            type_name::<T>(),
            err
        )))
    })
}

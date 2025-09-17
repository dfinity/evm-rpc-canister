//! Conversion between JSON types and Candid EVM RPC types.

use crate::rpc_client::{
    amount::Amount,
    json::{
        requests::{AccessList, AccessListItem, BlockSpec, EthCallParams, TransactionRequest},
        responses::Data,
        Hash, JsonByte, StorageKey,
    },
};
use evm_rpc_types::{BlockTag, Hex, Hex20, Hex256, Hex32, HexByte, Nat256};
use ic_ethereum_types::Address;

pub(super) fn into_block_spec(value: BlockTag) -> BlockSpec {
    use crate::rpc_client::json::requests;
    match value {
        BlockTag::Number(n) => BlockSpec::Number(n.into()),
        BlockTag::Latest => BlockSpec::Tag(requests::BlockTag::Latest),
        BlockTag::Safe => BlockSpec::Tag(requests::BlockTag::Safe),
        BlockTag::Finalized => BlockSpec::Tag(requests::BlockTag::Finalized),
        BlockTag::Earliest => BlockSpec::Tag(requests::BlockTag::Earliest),
        BlockTag::Pending => BlockSpec::Tag(requests::BlockTag::Pending),
    }
}

pub(super) fn into_get_logs_param(
    value: evm_rpc_types::GetLogsArgs,
) -> crate::rpc_client::json::requests::GetLogsParam {
    crate::rpc_client::json::requests::GetLogsParam {
        from_block: value.from_block.map(into_block_spec).unwrap_or_default(),
        to_block: value.to_block.map(into_block_spec).unwrap_or_default(),
        address: value
            .addresses
            .into_iter()
            .map(|address| ic_ethereum_types::Address::new(address.into()))
            .collect(),
        topics: value
            .topics
            .unwrap_or_default()
            .into_iter()
            .map(|topic| {
                topic
                    .into_iter()
                    .map(|t| crate::rpc_client::json::FixedSizeData::new(t.into()))
                    .collect()
            })
            .collect(),
    }
}

pub(super) fn from_log_entries(
    value: Vec<crate::rpc_client::json::responses::LogEntry>,
) -> Vec<evm_rpc_types::LogEntry> {
    value.into_iter().map(from_log_entry).collect()
}

fn from_log_entry(value: crate::rpc_client::json::responses::LogEntry) -> evm_rpc_types::LogEntry {
    let value1 = value.address;
    evm_rpc_types::LogEntry {
        address: evm_rpc_types::Hex20::from(value1.into_bytes()),
        topics: value
            .topics
            .into_iter()
            .map(|t| t.into_bytes().into())
            .collect(),
        data: value.data.0.into(),
        block_hash: value.block_hash.map(|x| x.into_bytes().into()),
        block_number: value.block_number.map(Nat256::from),
        transaction_hash: value.transaction_hash.map(|x| x.into_bytes().into()),
        transaction_index: value.transaction_index.map(Nat256::from),
        log_index: value.log_index.map(Nat256::from),
        removed: value.removed,
    }
}

pub(super) fn into_fee_history_params(
    value: evm_rpc_types::FeeHistoryArgs,
) -> crate::rpc_client::json::requests::FeeHistoryParams {
    crate::rpc_client::json::requests::FeeHistoryParams {
        block_count: value.block_count.into(),
        highest_block: into_block_spec(value.newest_block),
        reward_percentiles: value.reward_percentiles.unwrap_or_default(),
    }
}

pub(super) fn from_fee_history(
    value: crate::rpc_client::json::responses::FeeHistory,
) -> evm_rpc_types::FeeHistory {
    evm_rpc_types::FeeHistory {
        oldest_block: value.oldest_block.into(),
        base_fee_per_gas: value
            .base_fee_per_gas
            .into_iter()
            .map(Nat256::from)
            .collect(),
        gas_used_ratio: value.gas_used_ratio,
        reward: value
            .reward
            .into_iter()
            .map(|x| x.into_iter().map(Nat256::from).collect())
            .collect(),
    }
}

pub(super) fn into_get_transaction_count_params(
    value: evm_rpc_types::GetTransactionCountArgs,
) -> crate::rpc_client::json::requests::GetTransactionCountParams {
    crate::rpc_client::json::requests::GetTransactionCountParams {
        address: ic_ethereum_types::Address::new(value.address.into()),
        block: into_block_spec(value.block),
    }
}

pub(super) fn from_transaction_receipt(
    value: crate::rpc_client::json::responses::TransactionReceipt,
) -> evm_rpc_types::TransactionReceipt {
    let value1 = value.from;
    evm_rpc_types::TransactionReceipt {
        block_hash: Hex32::from(value.block_hash.into_bytes()),
        block_number: value.block_number.into(),
        effective_gas_price: value.effective_gas_price.into(),
        gas_used: value.gas_used.into(),
        cumulative_gas_used: value.cumulative_gas_used.into(),
        status: value.status.map(|v| match v {
            crate::rpc_client::json::responses::TransactionStatus::Success => Nat256::from(1_u8),
            crate::rpc_client::json::responses::TransactionStatus::Failure => Nat256::from(0_u8),
        }),
        root: value.root.map(Hash::into_bytes).map(Hex32::from),
        transaction_hash: Hex32::from(value.transaction_hash.into_bytes()),
        contract_address: value
            .contract_address
            .map(|address| Hex20::from(address.into_bytes())),
        from: evm_rpc_types::Hex20::from(value1.into_bytes()),
        logs: from_log_entries(value.logs),
        logs_bloom: Hex256::from(value.logs_bloom.into_bytes()),
        to: value.to.map(|address| Hex20::from(address.into_bytes())),
        transaction_index: value.transaction_index.into(),
        tx_type: HexByte::from(value.tx_type.into_byte()),
    }
}

pub(super) fn from_block(value: crate::rpc_client::json::responses::Block) -> evm_rpc_types::Block {
    let value1 = value.miner;
    evm_rpc_types::Block {
        base_fee_per_gas: value.base_fee_per_gas.map(Nat256::from),
        number: value.number.into(),
        difficulty: value.difficulty.map(Nat256::from),
        extra_data: Hex::from(value.extra_data.0),
        gas_limit: value.gas_limit.into(),
        gas_used: value.gas_used.into(),
        hash: Hex32::from(value.hash.into_bytes()),
        logs_bloom: Hex256::from(value.logs_bloom.into_bytes()),
        miner: evm_rpc_types::Hex20::from(value1.into_bytes()),
        mix_hash: Hex32::from(value.mix_hash.into_bytes()),
        nonce: value.nonce.into(),
        parent_hash: Hex32::from(value.parent_hash.into_bytes()),
        receipts_root: Hex32::from(value.receipts_root.into_bytes()),
        sha3_uncles: Hex32::from(value.sha3_uncles.into_bytes()),
        size: value.size.into(),
        state_root: Hex32::from(value.state_root.into_bytes()),
        timestamp: value.timestamp.into(),
        // The field totalDifficulty was removed from the official Ethereum JSON RPC Block schema in
        // https://github.com/ethereum/execution-apis/pull/570 and as a consequence is inconsistent between different providers.
        // See https://github.com/internet-computer-protocol/evm-rpc-canister/issues/311.
        total_difficulty: None,
        transactions: value
            .transactions
            .into_iter()
            .map(|tx| Hex32::from(tx.into_bytes()))
            .collect(),
        transactions_root: value.transactions_root.map(|x| Hex32::from(x.into_bytes())),
        uncles: value
            .uncles
            .into_iter()
            .map(|tx| Hex32::from(tx.into_bytes()))
            .collect(),
    }
}

pub(super) fn from_send_raw_transaction_result(
    transaction_hash: Option<Hex32>,
    value: crate::rpc_client::json::responses::SendRawTransactionResult,
) -> evm_rpc_types::SendRawTransactionStatus {
    match value {
        crate::rpc_client::json::responses::SendRawTransactionResult::Ok => {
            evm_rpc_types::SendRawTransactionStatus::Ok(transaction_hash)
        }
        crate::rpc_client::json::responses::SendRawTransactionResult::InsufficientFunds => {
            evm_rpc_types::SendRawTransactionStatus::InsufficientFunds
        }
        crate::rpc_client::json::responses::SendRawTransactionResult::NonceTooLow => {
            evm_rpc_types::SendRawTransactionStatus::NonceTooLow
        }
        crate::rpc_client::json::responses::SendRawTransactionResult::NonceTooHigh => {
            evm_rpc_types::SendRawTransactionStatus::NonceTooHigh
        }
    }
}

pub(super) fn into_eth_call_params(value: evm_rpc_types::CallArgs) -> EthCallParams {
    EthCallParams {
        transaction: into_transaction_request(value.transaction),
        block: into_block_spec(value.block.unwrap_or_default()),
    }
}

fn into_transaction_request(
    evm_rpc_types::TransactionRequest {
        tx_type,
        nonce,
        to,
        from,
        gas,
        value,
        input,
        gas_price,
        max_priority_fee_per_gas,
        max_fee_per_gas,
        max_fee_per_blob_gas,
        access_list,
        blob_versioned_hashes,
        blobs,
        chain_id,
    }: evm_rpc_types::TransactionRequest,
) -> TransactionRequest {
    fn map_access_list(list: evm_rpc_types::AccessList) -> AccessList {
        AccessList(
            list.0
                .into_iter()
                .map(|entry| AccessListItem {
                    address: Address::new(entry.address.into()),
                    storage_keys: entry
                        .storage_keys
                        .into_iter()
                        .map(|key| StorageKey::new(key.into()))
                        .collect(),
                })
                .collect(),
        )
    }
    TransactionRequest {
        tx_type: tx_type.map(|t| JsonByte::new(t.into())),
        nonce: nonce.map(Amount::from),
        to: to.map(|address| Address::new(address.into())),
        from: from.map(|address| Address::new(address.into())),
        gas: gas.map(Amount::from),
        value: value.map(Amount::from),
        input: input.map(into_data),
        gas_price: gas_price.map(Amount::from),
        max_priority_fee_per_gas: max_priority_fee_per_gas.map(Amount::from),
        max_fee_per_gas: max_fee_per_gas.map(Amount::from),
        max_fee_per_blob_gas: max_fee_per_blob_gas.map(Amount::from),
        access_list: access_list.map(map_access_list),
        blob_versioned_hashes: blob_versioned_hashes
            .map(|hashes| hashes.into_iter().map(into_hash).collect()),
        blobs: blobs.map(|blobs| blobs.into_iter().map(into_data).collect()),
        chain_id: chain_id.map(Amount::from),
    }
}

pub(super) fn into_hash(value: Hex32) -> Hash {
    Hash::new(value.into())
}

pub(super) fn from_data(value: Data) -> Hex {
    Hex::from(value.0)
}

fn into_data(value: Hex) -> Data {
    Data::from(Vec::<u8>::from(value))
}

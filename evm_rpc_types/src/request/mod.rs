use crate::{Hex, Hex20, Hex32, HexByte, Nat256, RpcError};
use candid::CandidType;
use serde::Deserialize;

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize, Default)]
pub enum BlockTag {
    #[default]
    Latest,
    Finalized,
    Safe,
    Earliest,
    Pending,
    Number(Nat256),
}

#[cfg(feature = "alloy")]
impl From<alloy_rpc_types::BlockNumberOrTag> for BlockTag {
    fn from(tag: alloy_rpc_types::BlockNumberOrTag) -> Self {
        use alloy_rpc_types::BlockNumberOrTag;
        match tag {
            BlockNumberOrTag::Latest => Self::Latest,
            BlockNumberOrTag::Finalized => Self::Finalized,
            BlockNumberOrTag::Safe => Self::Safe,
            BlockNumberOrTag::Earliest => Self::Earliest,
            BlockNumberOrTag::Pending => Self::Pending,
            BlockNumberOrTag::Number(n) => Self::Number(n.into()),
        }
    }
}

#[cfg(feature = "alloy")]
impl TryFrom<BlockTag> for alloy_rpc_types::BlockNumberOrTag {
    type Error = RpcError;

    fn try_from(tag: BlockTag) -> Result<Self, Self::Error> {
        Ok(match tag {
            BlockTag::Latest => Self::Latest,
            BlockTag::Finalized => Self::Finalized,
            BlockTag::Safe => Self::Safe,
            BlockTag::Earliest => Self::Earliest,
            BlockTag::Pending => Self::Pending,
            BlockTag::Number(n) => Self::Number(u64::try_from(n)?),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct FeeHistoryArgs {
    /// Number of blocks in the requested range.
    /// Typically, providers request this to be between 1 and 1024.
    #[serde(rename = "blockCount")]
    pub block_count: Nat256,

    /// Highest block of the requested range.
    /// Integer block number, or "latest" for the last mined block or "pending", "earliest" for not yet mined transactions.
    #[serde(rename = "newestBlock")]
    pub newest_block: BlockTag,

    /// A monotonically increasing list of percentile values between 0 and 100.
    /// For each block in the requested range, the transactions will be sorted in ascending order
    /// by effective tip per gas and the corresponding effective tip for the percentile
    /// will be determined, accounting for gas consumed.
    #[serde(rename = "rewardPercentiles")]
    pub reward_percentiles: Option<Vec<u8>>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct GetLogsArgs {
    /// Integer block number, or "latest" for the last mined block or "pending", "earliest" for not yet mined transactions.
    #[serde(rename = "fromBlock")]
    pub from_block: Option<BlockTag>,

    /// Integer block number, or "latest" for the last mined block or "pending", "earliest" for not yet mined transactions.
    #[serde(rename = "toBlock")]
    pub to_block: Option<BlockTag>,

    /// Contract address or a list of addresses from which logs should originate.
    pub addresses: Vec<Hex20>,

    /// Array of 32-byte DATA topics.
    /// Topics are order-dependent.
    /// Each topic can also be an array of DATA with "or" options.
    pub topics: Option<Vec<Vec<Hex32>>>,
}

#[cfg(feature = "alloy")]
impl From<Vec<alloy_primitives::Address>> for GetLogsArgs {
    fn from(addresses: Vec<alloy_primitives::Address>) -> Self {
        Self {
            from_block: None,
            to_block: None,
            addresses: addresses.into_iter().map(Hex20::from).collect(),
            topics: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct GetTransactionCountArgs {
    pub address: Hex20,
    pub block: BlockTag,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct CallArgs {
    pub transaction: TransactionRequest,
    /// Integer block number, or "latest" for the last mined block or "pending", "earliest" for not yet mined transactions.
    /// Default to "latest" if unspecified, see <https://github.com/ethereum/execution-apis/issues/461>.
    pub block: Option<BlockTag>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, CandidType, Deserialize)]
pub struct TransactionRequest {
    /// The type of the transaction:
    /// - "0x0" for legacy transactions (pre- EIP-2718)
    /// - "0x1" for access list transactions (EIP-2930)
    /// - "0x2" for EIP-1559 transactions
    #[serde(rename = "type")]
    pub tx_type: Option<HexByte>,

    /// Transaction nonce
    pub nonce: Option<Nat256>,

    /// Address of the receiver or `None` in a contract creation transaction.
    pub to: Option<Hex20>,

    /// The address of the sender.
    pub from: Option<Hex20>,

    /// Gas limit for the transaction.
    pub gas: Option<Nat256>,

    /// Amount of ETH sent with this transaction.
    pub value: Option<Nat256>,

    /// Transaction input data
    pub input: Option<Hex>,

    /// The legacy gas price willing to be paid by the sender in wei.
    #[serde(rename = "gasPrice")]
    pub gas_price: Option<Nat256>,

    /// Maximum fee per gas the sender is willing to pay to miners in wei.
    #[serde(rename = "maxPriorityFeePerGas")]
    pub max_priority_fee_per_gas: Option<Nat256>,

    /// The maximum total fee per gas the sender is willing to pay (includes the network / base fee and miner / priority fee) in wei.
    #[serde(rename = "maxFeePerGas")]
    pub max_fee_per_gas: Option<Nat256>,

    /// The maximum total fee per gas the sender is willing to pay for blob gas in wei.
    #[serde(rename = "maxFeePerBlobGas")]
    pub max_fee_per_blob_gas: Option<Nat256>,

    /// EIP-2930 access list
    #[serde(rename = "accessList")]
    pub access_list: Option<AccessList>,

    /// List of versioned blob hashes associated with the transaction's EIP-4844 data blobs.
    #[serde(rename = "blobVersionedHashes")]
    pub blob_versioned_hashes: Option<Vec<Hex32>>,

    /// Raw blob data.
    pub blobs: Option<Vec<Hex>>,

    /// Chain ID that this transaction is valid on.
    #[serde(rename = "chainId")]
    pub chain_id: Option<Nat256>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
#[serde(transparent)]
pub struct AccessList(pub Vec<AccessListEntry>);

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct AccessListEntry {
    pub address: Hex20,
    #[serde(rename = "storageKeys")]
    pub storage_keys: Vec<Hex32>,
}

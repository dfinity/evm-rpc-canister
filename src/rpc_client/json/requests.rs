use crate::rpc_client::json::responses::Data;
use crate::rpc_client::json::{FixedSizeData, Hash, JsonByte, StorageKey};
use crate::rpc_client::numeric::{
    BlockNumber, ChainId, GasAmount, NumBlocks, TransactionNonce, Wei, WeiPerGas,
};
use ic_ethereum_types::Address;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fmt::{Display, Formatter};

/// Parameters of the [`eth_getTransactionCount`](https://ethereum.org/en/developers/docs/apis/json-rpc/#eth_gettransactioncount) call.
#[derive(Debug, Serialize, Clone)]
#[serde(into = "(Address, BlockSpec)")]
pub struct GetTransactionCountParams {
    /// The address for which the transaction count is requested.
    pub address: Address,
    /// Integer block number, or "latest" for the last mined block or "pending", "earliest" for not yet mined transactions.
    pub block: BlockSpec,
}

impl From<GetTransactionCountParams> for (Address, BlockSpec) {
    fn from(params: GetTransactionCountParams) -> Self {
        (params.address, params.block)
    }
}

/// Parameters of the [`eth_getLogs`](https://ethereum.org/en/developers/docs/apis/json-rpc/#eth_getlogs) call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetLogsParam {
    /// Integer block number, or "latest" for the last mined block or "pending", "earliest" for not yet mined transactions.
    #[serde(rename = "fromBlock")]
    pub from_block: BlockSpec,
    /// Integer block number, or "latest" for the last mined block or "pending", "earliest" for not yet mined transactions.
    #[serde(rename = "toBlock")]
    pub to_block: BlockSpec,
    /// Contract address or a list of addresses from which logs should originate.
    pub address: Vec<Address>,
    /// Array of 32 Bytes DATA topics.
    /// Topics are order-dependent.
    /// Each topic can also be an array of DATA with "or" options.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub topics: Vec<Vec<FixedSizeData>>,
}

/// Parameters of the [`eth_feeHistory`](https://ethereum.github.io/execution-apis/api-documentation/) call.
#[derive(Debug, Serialize, Clone)]
#[serde(into = "(NumBlocks, BlockSpec, Vec<u8>)")]
pub struct FeeHistoryParams {
    /// Number of blocks in the requested range.
    /// Typically providers request this to be between 1 and 1024.
    pub block_count: NumBlocks,
    /// Highest block of the requested range.
    /// Integer block number, or "latest" for the last mined block or "pending", "earliest" for not yet mined transactions.
    pub highest_block: BlockSpec,
    /// A monotonically increasing list of percentile values between 0 and 100.
    /// For each block in the requested range, the transactions will be sorted in ascending order
    /// by effective tip per gas and the corresponding effective tip for the percentile
    /// will be determined, accounting for gas consumed.
    pub reward_percentiles: Vec<u8>,
}

impl From<FeeHistoryParams> for (NumBlocks, BlockSpec, Vec<u8>) {
    fn from(value: FeeHistoryParams) -> Self {
        (
            value.block_count,
            value.highest_block,
            value.reward_percentiles,
        )
    }
}

/// The block specification indicating which block to query.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum BlockSpec {
    /// Query the block with the specified index.
    Number(BlockNumber),
    /// Query the block with the specified tag.
    Tag(BlockTag),
}

impl Default for BlockSpec {
    fn default() -> Self {
        Self::Tag(BlockTag::default())
    }
}

impl From<BlockNumber> for BlockSpec {
    fn from(value: BlockNumber) -> Self {
        BlockSpec::Number(value)
    }
}

/// Block tags.
/// See <https://ethereum.org/en/developers/docs/apis/json-rpc/#default-block>
#[derive(Debug, Default, Copy, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum BlockTag {
    /// The latest mined block.
    #[default]
    #[serde(rename = "latest")]
    Latest,
    /// The latest safe head block.
    /// See
    /// <https://www.alchemy.com/overviews/ethereum-commitment-levels#what-are-ethereum-commitment-levels>
    #[serde(rename = "safe")]
    Safe,
    /// The latest finalized block.
    /// See
    /// <https://www.alchemy.com/overviews/ethereum-commitment-levels#what-are-ethereum-commitment-levels>
    #[serde(rename = "finalized")]
    Finalized,
    /// Earliest/genesis block
    #[serde(rename = "earliest")]
    Earliest,
    /// Pending state/transactions
    #[serde(rename = "pending")]
    Pending,
}

impl Display for BlockTag {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Latest => write!(f, "latest"),
            Self::Safe => write!(f, "safe"),
            Self::Finalized => write!(f, "finalized"),
            Self::Earliest => write!(f, "earliest"),
            Self::Pending => write!(f, "pending"),
        }
    }
}

/// Parameters of the [`eth_getBlockByNumber`](https://ethereum.org/en/developers/docs/apis/json-rpc/#eth_getblockbynumber) call.
#[derive(Debug, Serialize, Clone)]
#[serde(into = "(BlockSpec, bool)")]
pub struct GetBlockByNumberParams {
    /// Integer block number, or "latest" for the last mined block or "pending", "earliest" for not yet mined transactions.
    pub block: BlockSpec,
    /// If true, returns the full transaction objects. If false, returns only the hashes of the transactions.
    pub include_full_transactions: bool,
}

impl From<GetBlockByNumberParams> for (BlockSpec, bool) {
    fn from(value: GetBlockByNumberParams) -> Self {
        (value.block, value.include_full_transactions)
    }
}

#[derive(Debug, Serialize, Clone)]
#[serde(into = "(TransactionRequest, BlockSpec)")]
pub struct EthCallParams {
    pub transaction: TransactionRequest,
    pub block: BlockSpec,
}

impl From<EthCallParams> for (TransactionRequest, BlockSpec) {
    fn from(value: EthCallParams) -> Self {
        (value.transaction, value.block)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TransactionRequest {
    /// The type of the transaction (e.g. "0x0" for legacy transactions, "0x2" for EIP-1559 transactions)
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub tx_type: Option<JsonByte>,

    /// Transaction nonce
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nonce: Option<TransactionNonce>,

    /// Address of the receiver or `None` in a contract creation transaction.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<Address>,

    /// The address of the sender.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<Address>,

    /// Gas limit for the transaction.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gas: Option<GasAmount>,

    /// Amount of ETH sent with this transaction.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<Wei>,

    /// Transaction input data
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<Data>,

    /// The legacy gas price willing to be paid by the sender in wei.
    #[serde(rename = "gasPrice", skip_serializing_if = "Option::is_none")]
    pub gas_price: Option<WeiPerGas>,

    /// Maximum fee per gas the sender is willing to pay to miners in wei.
    #[serde(
        rename = "maxPriorityFeePerGas",
        skip_serializing_if = "Option::is_none"
    )]
    pub max_priority_fee_per_gas: Option<WeiPerGas>,

    /// The maximum total fee per gas the sender is willing to pay (includes the network / base fee and miner / priority fee) in wei.
    #[serde(rename = "maxFeePerGas", skip_serializing_if = "Option::is_none")]
    pub max_fee_per_gas: Option<WeiPerGas>,

    /// The maximum total fee per gas the sender is willing to pay for blob gas in wei.
    #[serde(rename = "maxFeePerBlobGas", skip_serializing_if = "Option::is_none")]
    pub max_fee_per_blob_gas: Option<WeiPerGas>,

    /// EIP-2930 access list
    #[serde(rename = "accessList", skip_serializing_if = "Option::is_none")]
    pub access_list: Option<AccessList>,

    /// List of versioned blob hashes associated with the transaction's EIP-4844 data blobs.
    #[serde(
        rename = "blobVersionedHashes",
        skip_serializing_if = "Option::is_none"
    )]
    pub blob_versioned_hashes: Option<Vec<Hash>>,

    /// Raw blob data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blobs: Option<Vec<Data>>,

    /// Chain ID that this transaction is valid on.
    #[serde(rename = "chainId", skip_serializing_if = "Option::is_none")]
    pub chain_id: Option<ChainId>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(transparent)]
pub struct AccessList(pub Vec<AccessListItem>);

impl AccessList {
    pub fn new() -> Self {
        Self(Vec::new())
    }
}

impl Default for AccessList {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AccessListItem {
    /// Accessed address
    pub address: Address,
    /// Accessed storage keys
    #[serde(rename = "storageKeys")]
    pub storage_keys: Vec<StorageKey>,
}

/// An envelope for all JSON-RPC requests.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JsonRpcRequest<T> {
    pub jsonrpc: String,
    pub method: String,
    pub id: u64,
    pub params: T,
}

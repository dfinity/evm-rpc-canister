use crate::rpc_client::eth_rpc::{HttpResponsePayload, ResponseTransform};
use crate::rpc_client::json::{FixedSizeData, Hash, JsonByte, LogsBloom};
use crate::rpc_client::numeric::{
    BlockNonce, BlockNumber, Difficulty, GasAmount, LogIndex, NumBytes, Timestamp,
    TransactionIndex, Wei, WeiPerGas,
};
use candid::Deserialize;
use ic_ethereum_types::Address;
use serde::Serialize;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct TransactionReceipt {
    /// The hash of the block containing the transaction.
    #[serde(rename = "blockHash")]
    pub block_hash: Hash,

    /// The number of the block containing the transaction.
    #[serde(rename = "blockNumber")]
    pub block_number: BlockNumber,

    /// The total base charge plus tip paid for each unit of gas
    #[serde(rename = "effectiveGasPrice")]
    pub effective_gas_price: WeiPerGas,

    /// The amount of gas used by this specific transaction alone
    #[serde(rename = "gasUsed")]
    pub gas_used: GasAmount,

    /// Status of the transaction.
    pub status: Option<TransactionStatus>,

    /// The hash of the transaction
    #[serde(rename = "transactionHash")]
    pub transaction_hash: Hash,

    /// The contract address created, if the transaction was a contract creation, otherwise null.
    #[serde(rename = "contractAddress")]
    pub contract_address: Option<Address>,

    /// Address of the sender.
    pub from: Address,

    /// An array of log objects that generated this transaction
    pub logs: Vec<LogEntry>,

    /// The bloom filter which is used to retrieve related logs
    #[serde(rename = "logsBloom")]
    pub logs_bloom: LogsBloom,

    /// Address of the receiver or null in a contract creation transaction.
    pub to: Option<Address>,

    /// The transactions index position in the block
    #[serde(rename = "transactionIndex")]
    pub transaction_index: TransactionIndex,

    /// The type of the transaction (e.g. "0x0" for legacy transactions, "0x2" for EIP-1559 transactions)
    #[serde(rename = "type")]
    pub tx_type: JsonByte,
}

impl HttpResponsePayload for TransactionReceipt {
    fn response_transform() -> Option<ResponseTransform> {
        Some(ResponseTransform::TransactionReceipt)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(try_from = "ethnum::u256", into = "ethnum::u256")]
pub enum TransactionStatus {
    /// Transaction was mined and executed successfully.
    Success,

    /// Transaction was mined but execution failed (e.g., out-of-gas error).
    /// The amount of the transaction is returned to the sender but gas is consumed.
    /// Note that this is different from a transaction that is not mined at all: a failed transaction
    /// is part of the blockchain and the next transaction from the same sender should have an incremented
    /// transaction nonce.
    Failure,
}

impl From<TransactionStatus> for ethnum::u256 {
    fn from(value: TransactionStatus) -> Self {
        match value {
            TransactionStatus::Success => ethnum::u256::ONE,
            TransactionStatus::Failure => ethnum::u256::ZERO,
        }
    }
}

impl TryFrom<ethnum::u256> for TransactionStatus {
    type Error = String;

    fn try_from(value: ethnum::u256) -> Result<Self, Self::Error> {
        match value {
            ethnum::u256::ZERO => Ok(TransactionStatus::Failure),
            ethnum::u256::ONE => Ok(TransactionStatus::Success),
            _ => Err(format!("invalid transaction status: {}", value)),
        }
    }
}

impl Display for TransactionStatus {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            TransactionStatus::Success => write!(f, "Success"),
            TransactionStatus::Failure => write!(f, "Failure"),
        }
    }
}

/// An entry of the [`eth_getLogs`](https://ethereum.org/en/developers/docs/apis/json-rpc/#eth_getlogs) call reply.
///
/// Example:
/// ```json
/// {
///    "address": "0x7e41257f7b5c3dd3313ef02b1f4c864fe95bec2b",
///    "topics": [
///      "0x2a2607d40f4a6feb97c36e0efd57e0aa3e42e0332af4fceb78f21b7dffcbd657"
///    ],
///    "data": "0x00000000000000000000000055654e7405fcb336386ea8f36954a211b2cda764000000000000000000000000000000000000000000000000002386f26fc100000000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000003f62327071372d71677a7a692d74623564622d72357363692d637736736c2d6e646f756c2d666f7435742d347a7732702d657a6677692d74616a32792d76716500",
///    "blockNumber": "0x3aa4f4",
///    "transactionHash": "0x5618f72c485bd98a3df58d900eabe9e24bfaa972a6fe5227e02233fad2db1154",
///    "transactionIndex": "0x6",
///    "blockHash": "0x908e6b84d26d71421bfaa08e7966e0afcef3883a28a53a0a7a31104caf1e94c2",
///    "logIndex": "0x8",
///    "removed": false
///  }
/// ```
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct LogEntry {
    /// The address from which this log originated.
    pub address: Address,
    /// Array of 0 to 4 32 Bytes DATA of indexed log arguments.
    /// In solidity: The first topic is the event signature hash (e.g. Deposit(address,bytes32,uint256)),
    /// unless you declared the event with the anonymous specifier.
    pub topics: Vec<FixedSizeData>,
    /// Contains one or more 32-byte non-indexed log arguments.
    pub data: Data,
    /// The block number in which this log appeared.
    /// None if the block is pending.
    #[serde(rename = "blockNumber")]
    pub block_number: Option<BlockNumber>,
    /// 32 Bytes - hash of the transactions from which this log was created.
    /// None when its pending log.
    #[serde(rename = "transactionHash")]
    pub transaction_hash: Option<Hash>,
    /// Integer of the transactions position within the block the log was created from.
    /// None if the log is pending.
    #[serde(rename = "transactionIndex")]
    pub transaction_index: Option<TransactionIndex>,
    /// 32 Bytes - hash of the block in which this log appeared.
    /// None if the block is pending.
    #[serde(rename = "blockHash")]
    pub block_hash: Option<Hash>,
    /// Integer of the log index position in the block.
    /// None if the log is pending.
    #[serde(rename = "logIndex")]
    pub log_index: Option<LogIndex>,
    /// "true" when the log was removed due to a chain reorganization.
    /// "false" if it's a valid log.
    #[serde(default)]
    pub removed: bool,
}

impl HttpResponsePayload for Vec<LogEntry> {
    fn response_transform() -> Option<ResponseTransform> {
        Some(ResponseTransform::LogEntries)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Block {
    /// Base fee per gas
    /// Only included for blocks after the London Upgrade / EIP-1559.
    #[serde(rename = "baseFeePerGas")]
    pub base_fee_per_gas: Option<Wei>,

    /// Block number
    pub number: BlockNumber,

    /// Difficulty
    pub difficulty: Option<Difficulty>,

    /// Extra data
    #[serde(rename = "extraData")]
    pub extra_data: Data,

    /// Maximum gas allowed in this block
    #[serde(rename = "gasLimit")]
    pub gas_limit: GasAmount,

    /// Gas used by all transactions in this block
    #[serde(rename = "gasUsed")]
    pub gas_used: GasAmount,

    /// Block hash
    pub hash: Hash,

    /// Bloom filter for the logs.
    #[serde(rename = "logsBloom")]
    pub logs_bloom: LogsBloom,

    /// Miner
    pub miner: Address,

    /// Mix hash
    #[serde(rename = "mixHash")]
    pub mix_hash: Hash,

    /// Nonce
    pub nonce: BlockNonce,

    /// Parent block hash
    #[serde(rename = "parentHash")]
    pub parent_hash: Hash,

    /// Receipts root
    #[serde(rename = "receiptsRoot")]
    pub receipts_root: Hash,

    /// Ommers hash
    #[serde(rename = "sha3Uncles")]
    pub sha3_uncles: Hash,

    /// Block size
    pub size: NumBytes,

    /// State root
    #[serde(rename = "stateRoot")]
    pub state_root: Hash,

    /// Timestamp
    #[serde(rename = "timestamp")]
    pub timestamp: Timestamp,

    /// List of transactions in the block.
    /// Note that since `eth_get_block_by_number` sets `include_full_transactions` to false,
    /// this field only contains the transaction hashes and not the full transactions.
    #[serde(default)]
    pub transactions: Vec<Hash>,

    /// Transactions root
    #[serde(rename = "transactionsRoot")]
    pub transactions_root: Option<Hash>,

    /// Uncles
    #[serde(default)]
    pub uncles: Vec<Hash>,
}

impl HttpResponsePayload for Block {
    fn response_transform() -> Option<ResponseTransform> {
        Some(ResponseTransform::Block)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FeeHistory {
    /// Lowest number block of the returned range.
    #[serde(rename = "oldestBlock")]
    pub oldest_block: BlockNumber,
    /// An array of block base fees per gas.
    /// This includes the next block after the newest of the returned range,
    /// because this value can be derived from the newest block.
    /// Zeroes are returned for pre-EIP-1559 blocks.
    #[serde(rename = "baseFeePerGas")]
    pub base_fee_per_gas: Vec<WeiPerGas>,
    /// An array of block gas used ratios (gasUsed / gasLimit).
    #[serde(default)]
    #[serde(rename = "gasUsedRatio")]
    pub gas_used_ratio: Vec<f64>,
    /// A two-dimensional array of effective priority fees per gas at the requested block percentiles.
    #[serde(default)]
    #[serde(rename = "reward")]
    pub reward: Vec<Vec<WeiPerGas>>,
}

impl HttpResponsePayload for FeeHistory {
    fn response_transform() -> Option<ResponseTransform> {
        Some(ResponseTransform::FeeHistory)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub enum SendRawTransactionResult {
    Ok,
    InsufficientFunds,
    NonceTooLow,
    NonceTooHigh,
}

impl HttpResponsePayload for SendRawTransactionResult {
    fn response_transform() -> Option<ResponseTransform> {
        Some(ResponseTransform::SendRawTransaction)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(transparent)]
pub struct Data(#[serde(with = "ic_ethereum_types::serde_data")] pub Vec<u8>);

impl HttpResponsePayload for Data {}

impl From<Vec<u8>> for Data {
    fn from(data: Vec<u8>) -> Self {
        Self(data)
    }
}

impl AsRef<[u8]> for Data {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

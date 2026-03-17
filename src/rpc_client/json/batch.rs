use crate::{
    rpc_client::{
        eth_rpc::ResponseTransform,
        json::{
            requests::{
                BlockSpec, EthCallParams, FeeHistoryParams, GetBlockByNumberParams, GetLogsParams,
                GetTransactionCountParams,
            },
            responses::{
                Block, Data, FeeHistory, LogEntry, SendRawTransactionResult, TransactionReceipt,
            },
            Hash,
        },
        numeric::TransactionCount,
    },
    types::RpcMethod,
};
use canhttp::http::json::JsonRpcResponse;
use evm_rpc_types::{JsonRpcError, RpcError, RpcResult, ValidationError};
use serde::Serialize;

/// Typed parameters for a single item in a batch JSON-RPC request.
/// Variant names mirror [`evm_rpc_types::BatchRequest`].
#[derive(Debug, Clone)]
#[allow(clippy::enum_variant_names)]
pub enum BatchRequestItemParams {
    EthCall(Box<EthCallParams>),
    EthFeeHistory(FeeHistoryParams),
    EthGetBlockByNumber(GetBlockByNumberParams),
    EthGetLogs(GetLogsParams),
    EthGetTransactionCount(GetTransactionCountParams),
    EthGetTransactionReceipt(Hash),
    EthSendRawTransaction(String),
}

impl BatchRequestItemParams {
    pub fn method(&self) -> RpcMethod {
        match self {
            Self::EthCall(_) => RpcMethod::EthCall,
            Self::EthFeeHistory(_) => RpcMethod::EthFeeHistory,
            Self::EthGetBlockByNumber(_) => RpcMethod::EthGetBlockByNumber,
            Self::EthGetLogs(_) => RpcMethod::EthGetLogs,
            Self::EthGetTransactionCount(_) => RpcMethod::EthGetTransactionCount,
            Self::EthGetTransactionReceipt(_) => RpcMethod::EthGetTransactionReceipt,
            Self::EthSendRawTransaction(_) => RpcMethod::EthSendRawTransaction,
        }
    }

    pub fn transform(&self) -> ResponseTransform {
        match self {
            Self::EthCall(_) => ResponseTransform::Call,
            Self::EthFeeHistory(_) => ResponseTransform::FeeHistory,
            Self::EthGetBlockByNumber(_) => ResponseTransform::GetBlockByNumber,
            Self::EthGetLogs(_) => ResponseTransform::GetLogs,
            Self::EthGetTransactionCount(_) => ResponseTransform::GetTransactionCount,
            Self::EthGetTransactionReceipt(_) => ResponseTransform::GetTransactionReceipt,
            Self::EthSendRawTransaction(_) => ResponseTransform::SendRawTransaction,
        }
    }

    pub fn serialize_params(&self) -> serde_json::Value {
        fn to_value(v: impl serde::Serialize) -> serde_json::Value {
            serde_json::to_value(v).expect("BUG: failed to serialize params")
        }
        match self {
            Self::EthCall(params) => to_value(params.as_ref()),
            Self::EthFeeHistory(params) => to_value(params),
            Self::EthGetBlockByNumber(params) => to_value(params),
            Self::EthGetLogs(params) => to_value(params),
            Self::EthGetTransactionCount(params) => to_value(params),
            Self::EthGetTransactionReceipt(hash) => to_value(hash),
            Self::EthSendRawTransaction(raw_tx) => to_value(raw_tx),
        }
    }

    pub fn deserialize_response(
        &self,
        response: JsonRpcResponse<serde_json::Value>,
    ) -> RpcResult<BatchResponse> {
        let value = response.into_result().map_err(|err| {
            RpcError::JsonRpcError(JsonRpcError {
                code: err.code,
                message: err.message,
            })
        })?;

        fn deser<T: serde::de::DeserializeOwned>(value: serde_json::Value) -> RpcResult<T> {
            serde_json::from_value(value).map_err(|e| {
                RpcError::ValidationError(ValidationError::Custom(format!(
                    "Failed to deserialize response: {e}"
                )))
            })
        }

        match self {
            Self::EthCall(_) => deser::<Data>(value).map(BatchResponse::EthCall),
            Self::EthFeeHistory(_) => {
                deser::<FeeHistory>(value).map(|v| BatchResponse::EthFeeHistory(Box::new(v)))
            }
            Self::EthGetBlockByNumber(_) => {
                deser::<Block>(value).map(|v| BatchResponse::EthGetBlockByNumber(Box::new(v)))
            }
            Self::EthGetLogs(_) => deser::<Vec<LogEntry>>(value).map(BatchResponse::EthGetLogs),
            Self::EthGetTransactionCount(_) => {
                deser::<TransactionCount>(value).map(BatchResponse::EthGetTransactionCount)
            }
            Self::EthGetTransactionReceipt(_) => deser::<Option<TransactionReceipt>>(value)
                .map(|v| BatchResponse::EthGetTransactionReceipt(Box::new(v))),
            Self::EthSendRawTransaction(_) => {
                deser::<SendRawTransactionResult>(value).map(BatchResponse::EthSendRawTransaction)
            }
        }
    }
}

impl From<evm_rpc_types::BatchRequest> for BatchRequestItemParams {
    fn from(request: evm_rpc_types::BatchRequest) -> Self {
        use evm_rpc_types::BatchRequest;
        match request {
            BatchRequest::EthCall(args) => Self::EthCall(Box::new(EthCallParams::from(*args))),
            BatchRequest::EthFeeHistory(args) => Self::EthFeeHistory(FeeHistoryParams::from(args)),
            BatchRequest::EthGetBlockByNumber(tag) => {
                Self::EthGetBlockByNumber(GetBlockByNumberParams {
                    block: BlockSpec::from(tag),
                    include_full_transactions: false,
                })
            }
            BatchRequest::EthGetLogs(batch_args) => {
                Self::EthGetLogs(GetLogsParams::from(batch_args.args))
            }
            BatchRequest::EthGetTransactionCount(args) => {
                Self::EthGetTransactionCount(GetTransactionCountParams::from(args))
            }
            BatchRequest::EthGetTransactionReceipt(tx_hash) => {
                Self::EthGetTransactionReceipt(Hash::from(tx_hash))
            }
            BatchRequest::EthSendRawTransaction(raw_tx) => {
                Self::EthSendRawTransaction(raw_tx.to_string())
            }
        }
    }
}

/// A batch request.
pub struct BatchRequestParams(Vec<BatchRequestItemParams>);

impl<I> FromIterator<I> for BatchRequestParams
where
    I: Into<BatchRequestItemParams>,
{
    fn from_iter<T: IntoIterator<Item = I>>(iter: T) -> Self {
        Self(iter.into_iter().map(Into::into).collect())
    }
}

impl BatchRequestParams {
    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = &BatchRequestItemParams> {
        self.0.iter()
    }
}

/// Typed response for a single item in a batch JSON-RPC response.
/// Variant names mirror [`BatchRequestItemParams`].
#[derive(Debug, Clone, PartialEq, Serialize)]
#[allow(clippy::enum_variant_names)]
pub enum BatchResponse {
    EthCall(Data),
    EthFeeHistory(Box<FeeHistory>),
    EthGetBlockByNumber(Box<Block>),
    EthGetLogs(Vec<LogEntry>),
    EthGetTransactionCount(TransactionCount),
    EthGetTransactionReceipt(Box<Option<TransactionReceipt>>),
    EthSendRawTransaction(SendRawTransactionResult),
}

impl From<BatchResponse> for evm_rpc_types::BatchResult {
    fn from(response: BatchResponse) -> Self {
        match response {
            BatchResponse::EthCall(data) => Self::EthCall(Ok(data.into())),
            BatchResponse::EthFeeHistory(fh) => Self::EthFeeHistory(Ok((*fh).into())),
            BatchResponse::EthGetBlockByNumber(block) => {
                Self::EthGetBlockByNumber(Box::new(Ok((*block).into())))
            }
            BatchResponse::EthGetLogs(entries) => Self::EthGetLogs(Ok(entries
                .into_iter()
                .map(evm_rpc_types::LogEntry::from)
                .collect())),
            BatchResponse::EthGetTransactionCount(count) => {
                Self::EthGetTransactionCount(Ok(count.into()))
            }
            BatchResponse::EthGetTransactionReceipt(receipt) => Self::EthGetTransactionReceipt(
                Box::new(Ok((*receipt).map(evm_rpc_types::TransactionReceipt::from))),
            ),
            BatchResponse::EthSendRawTransaction(result) => {
                Self::EthSendRawTransaction(Ok(result.into()))
            }
        }
    }
}

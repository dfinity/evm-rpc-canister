use crate::request::{EvmRpcRequest, EvmRpcResponseConverter, JsonRequest};
use evm_rpc_types::MultiRpcResult;

/// Defines [alloy](alloy.rs) response types.
pub struct AlloyResponseConverter;

impl EvmRpcResponseConverter for AlloyResponseConverter {
    type CallOutput = MultiRpcResult<alloy_primitives::Bytes>;
    type GetBlockByNumberOutput = MultiRpcResult<alloy_rpc_types::Block>;
    type GetLogsOutput = MultiRpcResult<Vec<alloy_rpc_types::Log>>;
    type GetTransactionCountOutput = MultiRpcResult<alloy_primitives::U256>;
    type GetTransactionReceiptOutput = MultiRpcResult<Option<alloy_rpc_types::TransactionReceipt>>;
    type FeeHistoryOutput = MultiRpcResult<alloy_rpc_types::FeeHistory>;
    type JsonRequestOutput = <JsonRequest as EvmRpcRequest>::CandidOutput;
    type SendRawTransactionOutput = MultiRpcResult<alloy_primitives::B256>;
}

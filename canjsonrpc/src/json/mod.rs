use serde::{Deserialize, Serialize};

/// An envelope for all JSON-RPC requests.
#[derive(Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest<T> {
    pub jsonrpc: String,
    pub method: String,
    pub id: u64,
    pub params: T,
}

impl<T> JsonRpcRequest<T> {
    pub fn new(id: u64, method: String, params: T) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            method,
            id,
            params,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JsonRpcResponse<T> {
    pub id: u64,
    pub jsonrpc: String,
    #[serde(flatten)]
    pub result: JsonRpcResult<T>,
}

/// An envelope for all JSON-RPC replies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum JsonRpcResult<T> {
    #[serde(rename = "result")]
    Result(T),
    #[serde(rename = "error")]
    Error { code: i64, message: String },
}

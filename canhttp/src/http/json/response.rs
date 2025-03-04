use crate::convert::Convert;
use crate::http::HttpResponse;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::marker::PhantomData;
use thiserror::Error;

pub type HttpJsonRpcResponse<T> = http::Response<JsonRpcResponseBody<T>>;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JsonRpcResponseBody<T> {
    pub jsonrpc: String,
    pub id: serde_json::Value,
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

#[derive(Error, Clone, Debug, Eq, PartialEq)]
pub enum JsonResponseConversionError {
    /// Response body could not be deserialized into a JSON-RPC response.
    #[error("Invalid HTTP JSON-RPC response: status {status}, body: {body}, parsing error: {parsing_error:?}"
    )]
    InvalidJsonResponse {
        status: u16,
        body: String,
        parsing_error: String,
    },
}

#[derive(Debug, Default)]
pub struct JsonResponseConverter<T> {
    _marker: PhantomData<T>,
}

impl<T> JsonResponseConverter<T> {
    pub fn new() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

// #[derive(Clone)] would otherwise introduce a bound T: Clone, which is not needed.
impl<T> Clone for JsonResponseConverter<T> {
    fn clone(&self) -> Self {
        Self {
            _marker: self._marker,
        }
    }
}

impl<T> Convert<HttpResponse> for JsonResponseConverter<T>
where
    T: DeserializeOwned,
{
    type Output = http::Response<T>;
    type Error = JsonResponseConversionError;

    fn try_convert(&mut self, response: HttpResponse) -> Result<Self::Output, Self::Error> {
        let (parts, body) = response.into_parts();
        let json_body: T = serde_json::from_slice(&body).map_err(|e| {
            JsonResponseConversionError::InvalidJsonResponse {
                status: parts.status.as_u16(),
                body: String::from_utf8_lossy(&body).to_string(),
                parsing_error: e.to_string(),
            }
        })?;
        Ok(http::Response::from_parts(parts, json_body))
    }
}

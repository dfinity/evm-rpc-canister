use crate::convert::Convert;
use crate::http::json::Id;
use crate::http::HttpResponse;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::marker::PhantomData;
use thiserror::Error;

/// Convert responses of type [HttpResponse] into [`http::Response<T>`],
/// where `T` can be deserialized.
#[derive(Debug)]
pub struct JsonResponseConverter<T> {
    _marker: PhantomData<T>,
}

impl<T> JsonResponseConverter<T> {
    /// Create a new instance of [`JsonResponseConverter`].
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

impl<T> Default for JsonResponseConverter<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Error returned when converting responses with [`JsonResponseConverter`].
#[derive(Error, Clone, Debug, Eq, PartialEq)]
pub enum JsonResponseConversionError {
    /// Response body could not be deserialized into a JSON-RPC response.
    #[error("Invalid HTTP JSON-RPC response: status {status}, body: {body}, parsing error: {parsing_error:?}"
    )]
    InvalidJsonResponse {
        /// Response status code
        status: u16,
        /// Response body
        body: String,
        /// Deserialization error
        parsing_error: String,
    },
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

/// JSON-RPC response over HTTP.
pub type HttpJsonRpcResponse<T> = http::Response<JsonRpcResponseBody<T>>;
pub type JsonRpcResult<T> = Result<T, JsonRpcError>;

/// Body of a JSON-RPC response
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JsonRpcResponseBody<T> {
    pub jsonrpc: String,
    id: Id,
    #[serde(flatten)]
    result: JsonRpcResultEnvelope<T>,
}

impl<T> JsonRpcResponseBody<T> {
    /// Creates a new successful response from a request ID and `Error` object.
    pub fn from_ok(id: Id, result: T) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            result: JsonRpcResultEnvelope::Ok(result),
            id,
        }
    }

    /// Creates a new error response from a request ID and `Error` object.
    pub fn from_error(id: Id, error: JsonRpcError) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            result: JsonRpcResultEnvelope::Err(error),
            id,
        }
    }

    pub fn from_parts(id: Id, result: JsonRpcResult<T>) -> Self {
        match result {
            Ok(r) => JsonRpcResponseBody::from_ok(id, r),
            Err(e) => JsonRpcResponseBody::from_error(id, e),
        }
    }

    pub fn into_parts(self) -> (Id, JsonRpcResult<T>) {
        (self.id, self.result.into_result())
    }

    pub fn into_result(self) -> JsonRpcResult<T> {
        self.result.into_result()
    }

    pub fn as_result_mut(&mut self) -> Result<&mut T, &mut JsonRpcError> {
        self.result.as_result_mut()
    }
}

/// An envelope for all JSON-RPC responses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum JsonRpcResultEnvelope<T> {
    /// Successful JSON-RPC response
    #[serde(rename = "result")]
    Ok(T),
    /// Failed JSON-RPC response
    #[serde(rename = "error")]
    Err(JsonRpcError),
}

impl<T> JsonRpcResultEnvelope<T> {
    pub fn into_result(self) -> JsonRpcResult<T> {
        match self {
            JsonRpcResultEnvelope::Ok(result) => Ok(result),
            JsonRpcResultEnvelope::Err(error) => Err(error),
        }
    }

    pub fn as_result_mut(&mut self) -> Result<&mut T, &mut JsonRpcError> {
        match self {
            JsonRpcResultEnvelope::Ok(result) => Ok(result),
            JsonRpcResultEnvelope::Err(error) => Err(error),
        }
    }
}

/// A JSON-RPC error object.
#[derive(Clone, Debug, Eq, PartialEq, Error, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[error("JSON-RPC error (code: {code}): {message}. Details: {data:?}")]
pub struct JsonRpcError {
    /// Indicate error type that occurred.
    pub code: i64,
    /// Short description of the error.
    pub message: String,
    /// Additional information about the error, if any.
    ///
    /// The value of this member is defined by the Server
    /// (e.g. detailed error information, nested errors etc.).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl JsonRpcError {
    pub fn new(code: impl Into<i64>, message: impl Into<String>) -> Self {
        let code = code.into();
        let message = message.into();
        Self {
            code,
            message,
            data: None,
        }
    }
}

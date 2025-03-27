use crate::convert::{Convert, CreateResponseFilter, Filter};
use crate::http::json::{HttpJsonRpcRequest, Id, Version};
use crate::http::HttpResponse;
use assert_matches::assert_matches;
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
pub type HttpJsonRpcResponse<T> = http::Response<JsonRpcResponse<T>>;

/// A specialized [`Result`] error type for JSON-RPC responses.
///
/// [`Result`]: enum@std::result::Result
pub type JsonRpcResult<T> = Result<T, JsonRpcError>;

/// JSON-RPC response.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct JsonRpcResponse<T> {
    jsonrpc: Version,
    id: Id,
    #[serde(flatten)]
    result: JsonRpcResultEnvelope<T>,
}

impl<T> JsonRpcResponse<T> {
    /// Creates a new successful response from a request ID and `Error` object.
    pub const fn from_ok(id: Id, result: T) -> Self {
        Self {
            jsonrpc: Version::V2,
            result: JsonRpcResultEnvelope::Ok(result),
            id,
        }
    }

    /// Creates a new error response from a request ID and `Error` object.
    pub const fn from_error(id: Id, error: JsonRpcError) -> Self {
        Self {
            jsonrpc: Version::V2,
            result: JsonRpcResultEnvelope::Err(error),
            id,
        }
    }

    /// Creates a new response from a request ID and either an `Ok(Value)` or `Err(Error)` body.
    pub fn from_parts(id: Id, result: JsonRpcResult<T>) -> Self {
        match result {
            Ok(r) => JsonRpcResponse::from_ok(id, r),
            Err(e) => JsonRpcResponse::from_error(id, e),
        }
    }

    /// Splits the response into a request ID paired with either an `Ok(Value)` or `Err(Error)` to
    /// signify whether the response is a success or failure.
    pub fn into_parts(self) -> (Id, JsonRpcResult<T>) {
        (self.id, self.result.into_result())
    }

    /// Similar to [`Self::into_parts`] but only takes a reference.
    pub fn as_parts(&self) -> (&Id, Result<&T, &JsonRpcError>) {
        (&self.id, self.as_result())
    }

    /// Convert this response into a result.
    ///
    /// A successful response will be converted to an `Ok` value,
    /// while a non-successful response will be converted into an `Err(JsonRpcError)`.
    pub fn into_result(self) -> JsonRpcResult<T> {
        self.result.into_result()
    }

    /// Similar to [`Self::into_result`] but only takes a reference.
    pub fn as_result(&self) -> Result<&T, &JsonRpcError> {
        self.result.as_result()
    }

    /// Mutate this response as a mutable result.
    pub fn as_result_mut(&mut self) -> Result<&mut T, &mut JsonRpcError> {
        self.result.as_result_mut()
    }

    /// Return the response ID.
    pub fn id(&self) -> &Id {
        &self.id
    }

    /// Map this response's result by the given function.
    #[allow(dead_code)]
    fn map(self, f: fn(T) -> T) -> Self {
        Self {
            result: self.result.map(f),
            ..self
        }
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
    fn into_result(self) -> JsonRpcResult<T> {
        match self {
            JsonRpcResultEnvelope::Ok(result) => Ok(result),
            JsonRpcResultEnvelope::Err(error) => Err(error),
        }
    }

    fn as_result(&self) -> Result<&T, &JsonRpcError> {
        match self {
            JsonRpcResultEnvelope::Ok(result) => Ok(result),
            JsonRpcResultEnvelope::Err(error) => Err(error),
        }
    }

    fn as_result_mut(&mut self) -> Result<&mut T, &mut JsonRpcError> {
        match self {
            JsonRpcResultEnvelope::Ok(result) => Ok(result),
            JsonRpcResultEnvelope::Err(error) => Err(error),
        }
    }

    #[allow(dead_code)]
    fn map(self, f: fn(T) -> T) -> Self {
        match self {
            JsonRpcResultEnvelope::Ok(result) => JsonRpcResultEnvelope::Ok(f(result)),
            JsonRpcResultEnvelope::Err(error) => JsonRpcResultEnvelope::Err(error),
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
    /// Create a new JSON-RPC error without `data`.
    pub fn new(code: impl Into<i64>, message: impl Into<String>) -> Self {
        let code = code.into();
        let message = message.into();
        Self {
            code,
            message,
            data: None,
        }
    }

    /// Return `true` if and only if the error code indicates a parsing error
    /// according to the [JSON-RPC specification](https://www.jsonrpc.org/specification).
    pub fn is_parse_error(&self) -> bool {
        self.code == -32700
    }

    /// Return `true` if and only if the error code indicates an invalid request
    /// according to the [JSON-RPC specification](https://www.jsonrpc.org/specification).
    pub fn is_invalid_request(&self) -> bool {
        self.code == -32600
    }
}

/// Error returned by the [`ConsistentJsonRpcIdFilter`].
#[derive(Error, Clone, Debug, Eq, PartialEq)]
pub enum ConsistentResponseIdFilterError {
    /// ID of the response does not match that of the request.
    #[error(
        "Unexpected identifier: expected response ID to be {request_id}, but got {response_id}"
    )]
    InconsistentId {
        /// Response status code.
        status: u16,
        /// ID from the request.
        request_id: Id,
        /// ID from the response.
        response_id: Id,
    },
}

/// Create [`ConsistentJsonRpcIdFilter`] for each request.
pub struct CreateJsonRpcIdFilter<I, O> {
    _marker: PhantomData<(I, O)>,
}

impl<I, O> CreateJsonRpcIdFilter<I, O> {
    /// Create a new instance of [`CreateJsonRpcIdFilter`]
    pub fn new() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

impl<I, O> Clone for CreateJsonRpcIdFilter<I, O> {
    fn clone(&self) -> Self {
        Self {
            _marker: self._marker,
        }
    }
}

impl<I, O> Default for CreateJsonRpcIdFilter<I, O> {
    fn default() -> Self {
        Self::new()
    }
}

impl<I, O> CreateResponseFilter<HttpJsonRpcRequest<I>, HttpJsonRpcResponse<O>>
    for CreateJsonRpcIdFilter<I, O>
{
    type Filter = ConsistentJsonRpcIdFilter<O>;
    type Error = ConsistentResponseIdFilterError;

    fn create_filter(&self, request: &HttpJsonRpcRequest<I>) -> ConsistentJsonRpcIdFilter<O> {
        ConsistentJsonRpcIdFilter::new(request.body().id().clone())
    }
}

/// Ensure that the ID of the response is consistent with the one from the request
/// that is stored internally.
pub struct ConsistentJsonRpcIdFilter<O> {
    request_id: Id,
    _marker: PhantomData<O>,
}

impl<O> ConsistentJsonRpcIdFilter<O> {
    /// Creates a new JSON-RPC filter to ensure that the ID of the response matches the one given in parameter.
    ///
    /// # Panics
    ///
    /// The method panics if the given ID is [`Id::Null`].
    /// This is because a request ID with value [`Id::Null`] indicates a Notification,
    /// which indicates that the client does not care about the response (see the
    /// JSON-RPC [specification](https://www.jsonrpc.org/specification)).
    pub fn new(request_id: Id) -> Self {
        assert_matches!(
            request_id,
            Id::Number(_) | Id::String(_),
            "ERROR: a null request ID is a notification that indicates that the client is not interested in the response."
        );
        Self {
            request_id,
            _marker: PhantomData,
        }
    }
}

impl<O> Filter<HttpJsonRpcResponse<O>> for ConsistentJsonRpcIdFilter<O> {
    type Error = ConsistentResponseIdFilterError;

    fn filter(
        &mut self,
        response: HttpJsonRpcResponse<O>,
    ) -> Result<HttpJsonRpcResponse<O>, Self::Error> {
        let request_id = &self.request_id;
        let (response_id, result) = response.body().as_parts();
        if request_id == response_id {
            return Ok(response);
        }

        if response_id.is_null()
            && result.is_err_and(|e| e.is_parse_error() || e.is_invalid_request())
        {
            // From the [JSON-RPC specification](https://www.jsonrpc.org/specification):
            // If there was an error in detecting the id in the Request object
            // (e.g. Parse error/Invalid Request), it MUST be Null.
            return Ok(response);
        }

        Err(ConsistentResponseIdFilterError::InconsistentId {
            status: response.status().as_u16(),
            request_id: request_id.clone(),
            response_id: response_id.clone(),
        })
    }
}

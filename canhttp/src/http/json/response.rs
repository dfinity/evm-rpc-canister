use crate::http::HttpResponse;
use pin_project::pin_project;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};
use thiserror::Error;
use tower::{BoxError, Service};
use tower_layer::Layer;

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

fn try_deserialize_response<T>(
    response: HttpResponse,
) -> Result<http::Response<T>, JsonResponseConversionError>
where
    T: DeserializeOwned,
{
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

// TODO XC-287: refactor to have a generic Response Filter mechanism
#[derive(Default)]
pub struct JsonResponseConversionLayer<T> {
    _marker: PhantomData<T>,
}

impl<T> JsonResponseConversionLayer<T> {
    pub fn new() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

pub struct JsonResponseConversion<S, T> {
    inner: S,
    _marker: PhantomData<T>,
}

impl<S, T> Layer<S> for JsonResponseConversionLayer<T> {
    type Service = JsonResponseConversion<S, T>;

    fn layer(&self, inner: S) -> Self::Service {
        Self::Service {
            inner,
            _marker: PhantomData,
        }
    }
}

impl<S, T, Request, Error> Service<Request> for JsonResponseConversion<S, T>
where
    S: Service<Request, Response = HttpResponse, Error = Error>,
    Error: Into<BoxError>,
    T: DeserializeOwned,
{
    type Response = http::Response<T>;
    type Error = BoxError;
    type Future = ResponseFuture<S::Future, T>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx).map_err(Into::into)
    }

    fn call(&mut self, req: Request) -> ResponseFuture<S::Future, T> {
        ResponseFuture {
            response_future: self.inner.call(req),
            _phantom_data: PhantomData::<T>,
        }
    }
}

#[pin_project]
pub struct ResponseFuture<F, T> {
    #[pin]
    response_future: F,
    _phantom_data: PhantomData<T>,
}

impl<F, E, T> Future for ResponseFuture<F, T>
where
    F: Future<Output = Result<HttpResponse, E>>,
    E: Into<BoxError>,
    T: DeserializeOwned,
{
    type Output = Result<http::Response<T>, BoxError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let result_fut = this.response_future.poll(cx);

        match result_fut {
            Poll::Ready(result) => match result {
                Ok(response) => {
                    Poll::Ready(try_deserialize_response::<T>(response).map_err(Into::into))
                }
                Err(e) => Poll::Ready(Err(e.into())),
            },
            Poll::Pending => Poll::Pending,
        }
    }
}

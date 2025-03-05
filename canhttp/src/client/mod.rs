#[cfg(test)]
mod tests;

use crate::convert::ConvertError;
use crate::{ConvertServiceBuilder, HttpsOutcallError};
use ic_cdk::api::call::RejectionCode;
use ic_cdk::api::management_canister::http_request::{
    CanisterHttpRequestArgument as IcHttpRequest, HttpResponse as IcHttpResponse,
};
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use thiserror::Error;
use tower::{BoxError, Service, ServiceBuilder};

/// Thin wrapper around [`ic_cdk::api::management_canister::http_request::http_request`]
/// that implements the [`tower::Service`] trait. Its functionality can be extended by composing so-called
/// [tower middlewares](https://docs.rs/tower/latest/tower/#usage).
///
/// Middlewares from this crate:
/// * [`crate::cycles::CyclesAccounting`]: handles cycles accounting.
/// * [`crate::observability`]: add logging or metrics.
/// * [`crate::http`]: use types from the [http](https://crates.io/crates/http) crate for requests and responses.
#[derive(Clone, Debug)]
pub struct Client;

impl Client {
    /// Create a new client returning custom errors.
    pub fn new_with_error<CustomError: From<IcError>>() -> ConvertError<Client, CustomError> {
        ServiceBuilder::new()
            .convert_error::<CustomError>()
            .service(Client)
    }

    /// Creates a new client where error type is erased.
    pub fn new_with_box_error(
    ) -> impl Service<IcHttpRequestWithCycles, Response = IcHttpResponse, Error = BoxError> {
        Self::new_with_error::<BoxError>()
    }
}

/// Error returned by the Internet Computer when making an HTTPs outcall.
#[derive(Error, Clone, Debug, PartialEq, Eq)]
#[error("Error from ICP: (code {code:?}, message {message})")]
pub struct IcError {
    /// Rejection code as specified [here](https://internetcomputer.org/docs/current/references/ic-interface-spec#reject-codes)
    pub code: RejectionCode,
    /// Associated helper message.
    pub message: String,
}

impl HttpsOutcallError for IcError {
    fn is_response_too_large(&self) -> bool {
        self.code == RejectionCode::SysFatal
            && (self.message.contains("size limit") || self.message.contains("length limit"))
    }
}

impl Service<IcHttpRequestWithCycles> for Client {
    type Response = IcHttpResponse;
    type Error = IcError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(
        &mut self,
        IcHttpRequestWithCycles { request, cycles }: IcHttpRequestWithCycles,
    ) -> Self::Future {
        Box::pin(async move {
            match ic_cdk::api::management_canister::http_request::http_request(request, cycles)
                .await
            {
                Ok((response,)) => Ok(response),
                Err((code, message)) => Err(IcError { code, message }),
            }
        })
    }
}

/// [`IcHttpRequest`] specifying how many cycles should be attached for the HTTPs outcall.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IcHttpRequestWithCycles {
    /// Request to be made.
    pub request: IcHttpRequest,
    /// Number of cycles to attach.
    pub cycles: u128,
}

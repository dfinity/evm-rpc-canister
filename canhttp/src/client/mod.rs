use ic_cdk::api::call::RejectionCode;
use ic_cdk::api::management_canister::http_request::{
    CanisterHttpRequestArgument as IcHttpRequest, HttpResponse as IcHttpResponse,
};
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use thiserror::Error;
use tower::Service;

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

/// Error returned by the Internet Computer when making an HTTPs outcall.
#[derive(Error, Clone, Debug, PartialEq, Eq)]
#[error("Error from ICP: (code {code:?}, message {message})")]
pub struct IcError {
    /// Rejection code as specified [here](https://internetcomputer.org/docs/current/references/ic-interface-spec#reject-codes)
    pub code: RejectionCode,
    /// Associated helper message.
    pub message: String,
}

impl IcError {
    /// Determines whether the error indicates that the response was larger than the specified
    /// [`max_response_bytes`](https://internetcomputer.org/docs/current/references/ic-interface-spec#ic-http_request) specified in the request.
    ///
    /// If true, retrying with a larger value for `max_response_bytes` may help.
    pub fn is_response_too_large(&self) -> bool {
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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IcHttpRequestWithCycles {
    pub request: IcHttpRequest,
    pub cycles: u128,
}

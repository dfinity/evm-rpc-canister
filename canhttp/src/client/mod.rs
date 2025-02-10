use ic_cdk::api::call::RejectionCode;
use ic_cdk::api::management_canister::http_request::{
    CanisterHttpRequestArgument as IcHttpRequest, HttpResponse as IcHttpResponse,
};
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use thiserror::Error;
use tower::{BoxError, Service};

#[derive(Clone)]
pub struct Client;

#[derive(Error, Debug, PartialEq, Eq)]
#[error("Error from ICP: (code {code:?}, message {message})")]
pub struct IcError {
    pub code: RejectionCode,
    pub message: String,
}

impl IcError {
    pub fn is_response_too_large(&self) -> bool {
        self.code == RejectionCode::SysFatal
            && (self.message.contains("size limit") || self.message.contains("length limit"))
    }
}

impl Service<IcHttpRequestWithCycles> for Client {
    type Response = IcHttpResponse;
    type Error = BoxError;
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
                Err((code, message)) => Err(BoxError::from(IcError { code, message })),
            }
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IcHttpRequestWithCycles {
    pub request: IcHttpRequest,
    pub cycles: u128,
}

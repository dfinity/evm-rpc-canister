use crate::cycles::{DefaultRequestCyclesCostEstimator, EstimateRequestCyclesCost};
use ic_cdk::api::call::RejectionCode;
use ic_cdk::api::management_canister::http_request::{
    CanisterHttpRequestArgument as IcHttpRequest, HttpResponse as IcHttpResponse,
};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use thiserror::Error;
use tower::{BoxError, Service};

#[derive(Clone)]
pub struct Client<CyclesEstimator> {
    cycles_estimator: Arc<CyclesEstimator>,
}

impl Client<DefaultRequestCyclesCostEstimator> {
    pub fn new(num_nodes: u32) -> Self {
        Self {
            cycles_estimator: Arc::new(DefaultRequestCyclesCostEstimator::new(num_nodes)),
        }
    }
}

#[derive(Error, Debug, PartialEq, Eq)]
#[error("Error from ICP: (code {code:?}, message {message})")]
pub struct IcError {
    code: RejectionCode,
    message: String,
}

impl IcError {
    pub fn is_response_too_large(&self) -> bool {
        &self.code == &RejectionCode::SysFatal
            && (self.message.contains("size limit") || self.message.contains("length limit"))
    }
}

impl<CyclesEstimator> Service<IcHttpRequest> for Client<CyclesEstimator>
where
    CyclesEstimator: EstimateRequestCyclesCost,
{
    type Response = IcHttpResponse;
    type Error = BoxError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: IcHttpRequest) -> Self::Future {
        let cycles_cost = self.cycles_estimator.cycles_cost(&request);
        Box::pin(async move {
            match ic_cdk::api::management_canister::http_request::http_request(
                request.clone(),
                cycles_cost,
            )
            .await
            {
                Ok((response,)) => Ok(response),
                Err((code, message)) => Err(BoxError::from(IcError { code, message })),
            }
        })
    }
}

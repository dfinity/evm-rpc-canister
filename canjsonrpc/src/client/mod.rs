use crate::cycles::{CyclesChargingStrategy, EstimateRequestCyclesCost};
use crate::http::{RequestBuilder, RequestError, ResponseError};
use crate::json::{JsonRpcRequest, JsonRpcResponse};
use ic_cdk::api::call::RejectionCode;
use ic_cdk::api::management_canister::http_request::{
    CanisterHttpRequestArgument, HttpMethod, HttpResponse,
};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::sync::Arc;

#[derive(Clone)]
pub struct Client {
    config: Arc<ClientConfig>,
}

pub struct ClientConfig {
    request_cost: Arc<dyn EstimateRequestCyclesCost>,
    charging: CyclesChargingStrategy,
    retry: Arc<dyn RetryStrategy>,
    cycles_cost_observer: Arc<dyn RequestObserver<u128>>,
    http_response_observer: Arc<dyn RequestObserver<HttpResponse>>,
    http_response_error_observer: Arc<dyn RequestObserver<IcError>>,
}

pub enum JsonRpcError {}

pub enum CallerError {
    InvalidUrl { reason: String },
}

pub enum HttpOutcallError {
    RequestError(RequestError),
    InsufficientCyclesError { expected: u128, received: u128 },
    IcError(IcError),
}

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

impl Client {
    pub async fn call<Params, Res>(
        &self,
        request: JsonRpcRequest<Params>,
        url: String,
    ) -> Result<JsonRpcResponse<Res>, JsonRpcError>
    where
        Params: Serialize,
        Res: DeserializeOwned,
    {
        // let request = CanisterHttpRequestArgument {
        //     url: url.clone(),
        //     max_response_bytes: Some(effective_size_estimate),
        //     method: HttpMethod::POST,
        //     headers: headers.clone(),
        //     body: Some(payload.as_bytes().to_vec()),
        //     transform: Some(TransformContext::from_name(
        //         "cleanup_response".to_owned(),
        //         transform_op,
        //     )),
        // };
        todo!()
    }

    pub fn post(&self, url: &str) -> RequestBuilder {
        RequestBuilder::new(self.clone(), HttpMethod::POST, url)
    }

    pub async fn execute_request(
        &self,
        request: CanisterHttpRequestArgument,
    ) -> Result<HttpResponse, HttpOutcallError> {
        let mut num_requests_sent = 0_u32;
        let mut request = request;
        loop {
            let cycles_cost = self.config.request_cost.cycles_cost(&request);
            match self.config.charging {
                CyclesChargingStrategy::PaidByCaller => {
                    let cycles_available = ic_cdk::api::call::msg_cycles_available128();
                    if cycles_available < cycles_cost {
                        return Err(HttpOutcallError::InsufficientCyclesError {
                            expected: cycles_cost,
                            received: cycles_available,
                        }
                        .into());
                    }
                    assert_eq!(
                        ic_cdk::api::call::msg_cycles_accept128(cycles_cost),
                        cycles_cost
                    );
                }
                CyclesChargingStrategy::PaidByCanister => {}
            };
            self.config
                .cycles_cost_observer
                .observe(&request, &cycles_cost);

            num_requests_sent += 1;
            match ic_cdk::api::management_canister::http_request::http_request(
                request.clone(),
                cycles_cost,
            )
            .await
            {
                Ok((response,)) => {
                    self.config
                        .http_response_observer
                        .observe(&request, &response);
                    return Ok(response);
                }
                Err((code, message)) => {
                    let error = IcError { code, message };
                    self.config
                        .http_response_error_observer
                        .observe(&request, &error);
                    match self
                        .config
                        .retry
                        .maybe_retry(num_requests_sent, request, &error)
                    {
                        Some(new_request) => request = new_request,
                        None => return Err(HttpOutcallError::IcError(error)),
                    }
                }
            }
        }
    }
}

/// Observe the request with some context.
///
/// Useful for metrics or logging purposes.
pub trait RequestObserver<T> {
    fn observe(&self, request: &CanisterHttpRequestArgument, value: &T);
}

struct RequestObserverNoOp;

impl<T> RequestObserver<T> for RequestObserverNoOp {
    fn observe(&self, _request: &CanisterHttpRequestArgument, _value: &T) {
        //NOP
    }
}

pub trait RetryStrategy {
    fn maybe_retry(
        &self,
        num_requests_sent: u32,
        previous_request: CanisterHttpRequestArgument,
        previous_error: &IcError,
    ) -> Option<CanisterHttpRequestArgument>;
}

struct NoRetry {}

impl RetryStrategy for NoRetry {
    fn maybe_retry(
        &self,
        _num_requests_sent: u32,
        _previous_request: CanisterHttpRequestArgument,
        _previous_error: &IcError,
    ) -> Option<CanisterHttpRequestArgument> {
        None
    }
}

/// Double the `max_response_bytes` in case the IC error indicates the response was too big.
struct DoubleMaxResponseBytes {}

impl RetryStrategy for DoubleMaxResponseBytes {
    fn maybe_retry(
        &self,
        _num_requests_sent: u32,
        previous_request: CanisterHttpRequestArgument,
        previous_error: &IcError,
    ) -> Option<CanisterHttpRequestArgument> {
        // This constant comes from the IC specification:
        // > If provided, the value must not exceed 2MB
        const HTTP_MAX_SIZE: u64 = 2_000_000;

        if previous_error.is_response_too_large() {
            if let Some(previous_estimate) = previous_request.max_response_bytes {
                let new_estimate = previous_estimate
                    .max(1024)
                    .saturating_mul(2)
                    .min(HTTP_MAX_SIZE);
                if new_estimate > previous_estimate {
                    return Some(CanisterHttpRequestArgument {
                        max_response_bytes: Some(new_estimate),
                        ..previous_request
                    });
                }
            }
        }
        None
    }
}

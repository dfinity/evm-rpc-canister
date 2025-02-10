use crate::cycles::{
    ChargeCaller, CyclesChargingStrategy, DefaultRequestCost, EstimateRequestCyclesCost,
};
use crate::http::{
    convert_response, IntoCanisterHttpRequest, RequestBuilder, RequestError, ResponseError,
};
use crate::json::{
    http_status_code, is_successful_http_code, CanisterJsonRpcRequestArgument, JsonRpcLayer,
    JsonRpcRequest, JsonRpcResponse,
};
use crate::retry::DoubleMaxResponseBytes;
use bytes::Bytes;
use http::Request;
use ic_cdk::api::call::{CallResult, RejectionCode};
use ic_cdk::api::management_canister::http_request::{
    CanisterHttpRequestArgument, HttpMethod, HttpResponse,
};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use thiserror::Error;
use tower::{BoxError, Layer, Service, ServiceBuilder, ServiceExt};

#[derive(Clone)]
pub struct Client<CyclesEstimator> {
    cycles_estimator: Arc<CyclesEstimator>,
}

pub enum JsonRpcError {}

pub enum CallerError {
    InvalidUrl { reason: String },
}

#[derive(Error, Debug)]
pub enum HttpOutcallError {
    #[error("insufficient cycles (expected {expected:?}, received {received:?})")]
    InsufficientCyclesError { expected: u128, received: u128 },
    #[error("{0}")]
    IcError(IcError),
}

#[derive(Error, Debug)]
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

impl Client<DefaultRequestCost> {
    pub fn new(num_nodes: u32) -> Self {
        Self {
            cycles_estimator: Arc::new(DefaultRequestCost::new(num_nodes)),
        }
    }
}

impl<E> Client<E> {
    pub fn http(num_nodes: u32) -> impl Service<http::Request<Bytes>> {
        let request_cost_estimator = DefaultRequestCost::new(num_nodes);
        ServiceBuilder::new()
            .check_service::<Client<DefaultRequestCost>, CanisterHttpRequestArgument, HttpResponse, HttpOutcallError>()
            .map_request(|r: http::Request<Bytes>| r.into_canister_http_request())
            .map_response(|response: HttpResponse| convert_response(response))
            .check_service::<Client<DefaultRequestCost>, http::Request<Bytes>, http::Response<Bytes>, HttpOutcallError>(
            )
            .service(
                ServiceBuilder::new()
                    .filter(ChargeCaller::new(request_cost_estimator))
                    .service(Client::new(num_nodes)),
            )
    }

    pub fn json_rpc(
        num_nodes: u32,
    ) -> impl Service<http::Request<JsonRpcRequest<serde_json::Value>>> {
        let request_cost_estimator = DefaultRequestCost::new(num_nodes);
        ServiceBuilder::new()
            .layer(JsonRpcLayer)
            .map_request(|r: http::Request<Bytes>| r.into_canister_http_request())
            .map_response(|response: HttpResponse| convert_response(response))
            .service(
                ServiceBuilder::new()
                    .filter(ChargeCaller::new(request_cost_estimator))
                    .service(Client::new(num_nodes)),
            )
    }

    // pub fn json_rpc2(
    //     num_nodes: u32,
    // ) -> impl Service<http::Request<JsonRpcRequest<serde_json::Value>>> {
    //     ServiceBuilder::new()
    //         .service(Self::http(num_nodes))
    //         .map_request(
    //             |request: http::Request<JsonRpcRequest<serde_json::Value>>| {
    //                 let (parts, body) = request.into_parts();
    //                 let serialized_body = Bytes::from(serde_json::to_vec(&body).unwrap());
    //                 http::Request::from_parts(parts, serialized_body)
    //             },
    //         )
    //         .map_result(|result: Result<http::Response<Bytes>, HttpOutcallError>| {
    //             match result {
    //                 Ok(response) => {
    //                     // JSON-RPC responses over HTTP should have a 2xx status code,
    //                     // even if the contained JsonRpcResult is an error.
    //                     // If the server is not available, it will sometimes (wrongly) return HTML that will fail parsing as JSON.
    //                     if !response.status().is_success() {
    //                         return Err(BoxError::from(
    //                             crate::json::JsonRpcError::InvalidHttpJsonRpcResponse {
    //                                 status: response.status().as_u16(),
    //                                 body: String::from_utf8_lossy(&response.into_body())
    //                                     .to_string(),
    //                                 parsing_error: None,
    //                             },
    //                         ));
    //                     }
    //                     let (parts, body) = response.into_parts();
    //                     let deser_body =
    //                         serde_json::from_slice::<JsonRpcResponse<serde_json::Value>>(
    //                             body.as_ref(),
    //                         )
    //                         .map_err(|e| {
    //                             BoxError::from(
    //                                 crate::json::JsonRpcError::InvalidHttpJsonRpcResponse {
    //                                     status: parts.status.as_u16(),
    //                                     body: String::from_utf8_lossy(body.as_ref()).to_string(),
    //                                     parsing_error: Some(e.to_string()),
    //                                 },
    //                             )
    //                         })?;
    //                     Ok(http::Response::from_parts(parts, deser_body))
    //                 }
    //                 Err(e) => Err(BoxError::from(e)),
    //             }
    //         })
    // }

    // pub fn new2(num_nodes: u32) -> impl Service<CanisterHttpRequestArgument> {
    //     let request_cost_estimator = DefaultRequestCost::new(num_nodes);
    //     ServiceBuilder::new()
    //         .filter(ChargeCaller::new(request_cost_estimator))
    //         .retry(DoubleMaxResponseBytes {})
    //         .service(Client::new(num_nodes))
    // }
    //
    // pub fn new3(num_nodes: u32) -> impl Service<CanisterJsonRpcRequestArgument> {
    //     let request_cost_estimator = DefaultRequestCost::new(num_nodes);
    //     ServiceBuilder::new()
    //         .filter(ChargeCaller::new(request_cost_estimator))
    //         .retry(DoubleMaxResponseBytes {})
    //         .service(Client::new(num_nodes))
    //         .map_request(|req: CanisterJsonRpcRequestArgument| {
    //             CanisterHttpRequestArgument::try_from(req).unwrap()
    //         })
    //         .map_result(|result| {
    //             match result {
    //                 Ok(response) => {
    //                     // JSON-RPC responses over HTTP should have a 2xx status code,
    //                     // even if the contained JsonRpcResult is an error.
    //                     // If the server is not available, it will sometimes (wrongly) return HTML that will fail parsing as JSON.
    //                     let http_status_code = http_status_code(&response);
    //                     if !is_successful_http_code(&http_status_code) {
    //                         return Err(BoxError::from(
    //                             crate::json::JsonRpcError::InvalidHttpJsonRpcResponse {
    //                                 status: http_status_code,
    //                                 body: String::from_utf8_lossy(&response.body).to_string(),
    //                                 parsing_error: None,
    //                             },
    //                         ));
    //                     }
    //                     serde_json::from_slice::<JsonRpcResponse<serde_json::Value>>(&response.body)
    //                         .map_err(|e| {
    //                             BoxError::from(
    //                                 crate::json::JsonRpcError::InvalidHttpJsonRpcResponse {
    //                                     status: http_status_code,
    //                                     body: String::from_utf8_lossy(&response.body).to_string(),
    //                                     parsing_error: Some(e.to_string()),
    //                                 },
    //                             )
    //                         })
    //                 }
    //                 Err(e) => Err(e),
    //             }
    //         })
    // }

    // pub async fn call<Params, Res>(
    //     &self,
    //     request: JsonRpcRequest<Params>,
    //     url: String,
    // ) -> Result<JsonRpcResponse<Res>, JsonRpcError>
    // where
    //     Params: Serialize,
    //     Res: DeserializeOwned,
    // {
    //     let request = CanisterHttpRequestArgument {
    //         url: url.clone(),
    //         max_response_bytes: Some(effective_size_estimate),
    //         method: HttpMethod::POST,
    //         headers: headers.clone(),
    //         body: Some(payload.as_bytes().to_vec()),
    //         transform: Some(TransformContext::from_name(
    //             "cleanup_response".to_owned(),
    //             transform_op,
    //         )),
    //     };
    //     todo!()
    // }

    // pub fn post(&self, url: &str) -> RequestBuilder {
    //     RequestBuilder::new(self.clone(), HttpMethod::POST, url)
    // }
    //
    // pub async fn execute_request(
    //     &self,
    //     request: CanisterHttpRequestArgument,
    // ) -> Result<HttpResponse, HttpOutcallError> {
    //     let mut num_requests_sent = 0_u32;
    //     let mut request = request;
    //     loop {
    //         let cycles_cost = self.config.request_cost.cycles_cost(&request);
    //         match self.config.charging {
    //             CyclesChargingStrategy::PaidByCaller => {
    //                 let cycles_available = ic_cdk::api::call::msg_cycles_available128();
    //                 if cycles_available < cycles_cost {
    //                     return Err(HttpOutcallError::InsufficientCyclesError {
    //                         expected: cycles_cost,
    //                         received: cycles_available,
    //                     }
    //                     .into());
    //                 }
    //                 assert_eq!(
    //                     ic_cdk::api::call::msg_cycles_accept128(cycles_cost),
    //                     cycles_cost
    //                 );
    //             }
    //             CyclesChargingStrategy::PaidByCanister => {}
    //         };
    //         self.config
    //             .cycles_cost_observer
    //             .observe(&request, &cycles_cost);
    //
    //         num_requests_sent += 1;
    //         match ic_cdk::api::management_canister::http_request::http_request(
    //             request.clone(),
    //             cycles_cost,
    //         )
    //         .await
    //         {
    //             Ok((response,)) => {
    //                 self.config
    //                     .http_response_observer
    //                     .observe(&request, &response);
    //                 return Ok(response);
    //             }
    //             Err((code, message)) => {
    //                 let error = IcError { code, message };
    //                 self.config
    //                     .http_response_error_observer
    //                     .observe(&request, &error);
    //                 match self
    //                     .config
    //                     .retry
    //                     .maybe_retry(num_requests_sent, request, &error)
    //                 {
    //                     Some(new_request) => request = new_request,
    //                     None => return Err(HttpOutcallError::IcError(error)),
    //                 }
    //             }
    //         }
    //     }
    // }
}

pub trait RequestExecutor<Request> {
    type Response;
    type Error;
    type Future: Future<Output = Result<Self::Response, Self::Error>>;

    #[must_use = "futures do nothing unless you `.await` or poll them"]
    fn send(&mut self, req: Request) -> Self::Future;
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

impl<CyclesEstimator> Service<CanisterHttpRequestArgument> for Client<CyclesEstimator>
where
    CyclesEstimator: EstimateRequestCyclesCost,
{
    type Response = HttpResponse;
    type Error = HttpOutcallError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: CanisterHttpRequestArgument) -> Self::Future {
        let cycles_cost = self.cycles_estimator.cycles_cost(&request);
        Box::pin(async move {
            match ic_cdk::api::management_canister::http_request::http_request(
                request.clone(),
                cycles_cost,
            )
            .await
            {
                Ok((response,)) => Ok(response),
                Err((code, message)) => Err(HttpOutcallError::IcError(IcError { code, message })),
            }
        })
    }
}

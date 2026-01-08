use crate::add_metric_entry;
use crate::http::error::HttpClientError;
use crate::http::{charging_policy_with_collateral, client, service_request_builder};
use crate::memory::{get_override_provider, record_ok_result};
use crate::providers::{resolve_rpc_service, SupportedRpcService};
use crate::rpc_client::eth_rpc::{ResponseSizeEstimate, ResponseTransformEnvelope};
use crate::rpc_client::IcHttpRequest;
use crate::types::MetricRpcService;
use crate::types::{MetricRpcMethod, ResolvedRpcService, RpcMethod};
use canhttp::cycles::CyclesChargingPolicy;
use canhttp::http::json::{HttpJsonRpcRequest, HttpJsonRpcResponse, JsonRpcRequest};
use canhttp::multi::{
    MultiResults, Reduce, ReduceWithEquality, ReduceWithThreshold, ReducedResult, ReductionError,
    Timestamp,
};
use canhttp::MaxResponseBytesRequestExtension;
use canhttp::TransformContextRequestExtension;
use evm_rpc_types::{
    ConsensusStrategy, JsonRpcError, MultiRpcResult, RpcError, RpcResult, RpcService,
};
use http::Response;
use ic_management_canister_types::{TransformContext, TransformFunc};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::collections::BTreeSet;
use std::fmt::Debug;
use tower::ServiceExt;

#[cfg(test)]
mod tests;

pub struct MultiRpcRequest<Params, Output> {
    providers: BTreeSet<RpcService>,
    method: RpcMethod,
    params: Params,
    response_size_estimate: ResponseSizeEstimate,
    transform: ResponseTransformEnvelope,
    reduction_strategy: ReductionStrategy,
    _marker: std::marker::PhantomData<Output>,
}

impl<Params, Output> MultiRpcRequest<Params, Output> {
    pub fn new(
        providers: BTreeSet<RpcService>,
        method: RpcMethod,
        params: Params,
        response_size_estimate: ResponseSizeEstimate,
        transform: impl Into<ResponseTransformEnvelope>,
        reduction_strategy: ReductionStrategy,
    ) -> MultiRpcRequest<Params, Output> {
        MultiRpcRequest {
            providers,
            method,
            params,
            response_size_estimate,
            transform: transform.into(),
            reduction_strategy,
            _marker: Default::default(),
        }
    }
}

impl<Params, Output> MultiRpcRequest<Params, Output> {
    pub async fn send_and_reduce(self) -> MultiRpcResult<Output>
    where
        Params: Serialize + Clone + Debug,
        Output: Debug + Serialize + DeserializeOwned + PartialEq,
    {
        let result = self.parallel_call().await.reduce(self.reduction_strategy);
        process_result(self.method, result)
    }

    /// Query all providers in parallel and return all results.
    /// It's up to the caller to decide how to handle the results, which could be inconsistent
    /// (e.g., if different providers gave different responses).
    /// This method is useful for querying data that is critical for the system to ensure that there is no single point of failure,
    /// e.g., ethereum logs upon which ckETH will be minted.
    async fn parallel_call(&self) -> MultiResults<RpcService, Output, RpcError>
    where
        Params: Serialize + Clone + Debug,
        Output: Debug + DeserializeOwned,
    {
        let requests = self.create_json_rpc_requests();

        let client = client(true).map_result(extract_json_rpc_response);

        let (requests, errors) = requests.into_inner();
        let (_client, mut results) = canhttp::multi::parallel_call(client, requests).await;
        results.add_errors(errors);
        let now = Timestamp::from_nanos_since_unix_epoch(ic_cdk::api::time());
        results
            .ok_results()
            .keys()
            .filter_map(SupportedRpcService::new)
            .for_each(|service| record_ok_result(service, now));
        assert_eq!(
            results.len(),
            self.providers.len(),
            "BUG: expected 1 result per provider"
        );
        results
    }

    /// Estimate the exact cycles cost for the given request.
    ///
    /// *IMPORTANT*: the method is *synchronous* in a canister environment.
    pub async fn cycles_cost(&self) -> RpcResult<u128>
    where
        Params: Serialize + Clone + Debug,
    {
        async fn extract_request(
            request: IcHttpRequest,
        ) -> Result<Response<IcHttpRequest>, HttpClientError> {
            Ok(Response::new(request))
        }

        let requests = self.create_json_rpc_requests();

        let client = service_request_builder()
            .service_fn(extract_request)
            .map_err(RpcError::from)
            .map_response(Response::into_body);

        let (requests, errors) = requests.into_inner();
        if let Some(error) = errors.into_values().next() {
            return Err(error);
        }

        let (_client, results) = canhttp::multi::parallel_call(client, requests).await;
        let (requests, errors) = results.into_inner();
        if !errors.is_empty() {
            return Err(errors
                .into_values()
                .next()
                .expect("BUG: errors is not empty"));
        }
        assert_eq!(
            requests.len(),
            self.providers.len(),
            "BUG: expected 1 result per provider"
        );

        let mut cycles_to_attach = 0_u128;

        let policy = charging_policy_with_collateral();
        for request in requests.into_values() {
            let request_cycles_cost = ic_cdk::management_canister::cost_http_request(&request);
            cycles_to_attach += policy.cycles_to_charge(&request, request_cycles_cost)
        }
        Ok(cycles_to_attach)
    }

    fn create_json_rpc_requests(
        &self,
    ) -> MultiResults<RpcService, HttpJsonRpcRequest<Params>, RpcError>
    where
        Params: Clone,
    {
        let transform_op = {
            let mut buf = vec![];
            minicbor::encode(&self.transform, &mut buf).unwrap();
            buf
        };
        let effective_size_estimate = self.response_size_estimate.get();
        let mut requests = MultiResults::default();
        for provider in self.providers.iter() {
            let request = resolve_rpc_service(provider.clone())
                .map_err(RpcError::from)
                .and_then(|rpc_service| rpc_service.post(&get_override_provider()))
                .map(|builder| {
                    builder
                        .max_response_bytes(effective_size_estimate)
                        .transform_context(TransformContext {
                            function: TransformFunc(candid::Func {
                                method: "cleanup_response".to_string(),
                                principal: ic_cdk::api::canister_self(),
                            }),
                            context: transform_op.clone(),
                        })
                        .body(JsonRpcRequest::new(
                            self.method.clone().name(),
                            self.params.clone(),
                        ))
                        .expect("BUG: invalid request")
                })
                .map(|mut request| {
                    // Store the original `RpcService` for usage when recording metrics
                    request.extensions_mut().insert(provider.clone());
                    // Store `MetricRpcMethod` for usage when recording metrics, which cannot simply
                    // later be determined from the JSON-RPC request method since we distinguish
                    // manual requests.
                    request
                        .extensions_mut()
                        .insert(MetricRpcMethod::from(self.method.clone()));
                    request
                });
            requests.insert_once(provider.clone(), request);
        }
        requests
    }
}

fn extract_json_rpc_response<O>(result: RpcResult<HttpJsonRpcResponse<O>>) -> RpcResult<O> {
    match result?.into_body().into_result() {
        Ok(value) => Ok(value),
        Err(json_rpc_error) => Err(RpcError::JsonRpcError(JsonRpcError {
            code: json_rpc_error.code,
            message: json_rpc_error.message,
        })),
    }
}

fn process_result<T>(
    method: impl Into<MetricRpcMethod> + Clone,
    result: ReducedResult<RpcService, T, RpcError>,
) -> MultiRpcResult<T> {
    match result {
        Ok(value) => MultiRpcResult::Consistent(Ok(value)),
        Err(err) => match err {
            ReductionError::ConsistentError(err) => MultiRpcResult::Consistent(Err(err)),
            ReductionError::InconsistentResults(multi_call_results) => {
                let results: Vec<_> = multi_call_results.into_iter().collect();
                results.iter().for_each(|(service, _service_result)| {
                    if let Ok(ResolvedRpcService::Provider(provider)) =
                        resolve_rpc_service(service.clone())
                    {
                        add_metric_entry!(
                            inconsistent_responses,
                            (
                                method.clone().into(),
                                MetricRpcService {
                                    host: provider
                                        .hostname()
                                        .unwrap_or_else(|| "(unknown)".to_string()),
                                    is_supported: !matches!(service, RpcService::Custom(_))
                                }
                            ),
                            1
                        )
                    }
                });
                MultiRpcResult::Inconsistent(results)
            }
        },
    }
}

pub enum ReductionStrategy {
    ByEquality(ReduceWithEquality),
    ByThreshold(ReduceWithThreshold),
}

impl From<ConsensusStrategy> for ReductionStrategy {
    fn from(value: ConsensusStrategy) -> Self {
        match value {
            ConsensusStrategy::Equality => ReductionStrategy::ByEquality(ReduceWithEquality),
            ConsensusStrategy::Threshold { total: _, min } => {
                ReductionStrategy::ByThreshold(ReduceWithThreshold::new(min))
            }
        }
    }
}

impl<T: PartialEq + Serialize> Reduce<RpcService, T, RpcError> for ReductionStrategy {
    fn reduce(
        &self,
        results: MultiResults<RpcService, T, RpcError>,
    ) -> ReducedResult<RpcService, T, RpcError> {
        match self {
            ReductionStrategy::ByEquality(r) => r.reduce(results),
            ReductionStrategy::ByThreshold(r) => r.reduce(results),
        }
    }
}

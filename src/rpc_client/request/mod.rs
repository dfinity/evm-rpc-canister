use crate::{
    add_metric_entry,
    http::{
        charging_policy_with_collateral, error::HttpClientError, http_client,
        service_request_builder,
    },
    memory::{get_override_provider, record_ok_result},
    providers::{resolve_rpc_service, SupportedRpcService},
    rpc_client::eth_rpc::{ResponseSizeEstimate, ResponseTransform},
    types::{MetricRpcMethod, MetricRpcService, ResolvedRpcService, RpcMethod},
};
use canhttp::{
    cycles::CyclesChargingPolicy,
    multi::{
        MultiResults, Reduce, ReduceWithEquality, ReduceWithThreshold, ReducedResult,
        ReductionError, Timestamp,
    },
    http::json::{JsonRpcRequest, JsonRpcResponse},
};
use evm_rpc_types::{
    ConsensusStrategy, JsonRpcError, MultiRpcResult, RpcError, RpcResult, RpcService,
};
use http::{Request as HttpRequest, Response as HttpResponse};
use ic_management_canister_types::{
    HttpRequestArgs as IcHttpRequest, TransformContext, TransformFunc,
};
use serde::Serialize;
use std::{collections::BTreeSet, marker::PhantomData};
use tower::ServiceExt;

pub type MultiProviderSingleJsonRpcCall<Request, Response> = MultiProviderJsonRpcCall<SingleJsonRpcCall<Request, Response>>;

pub struct MultiProviderJsonRpcCall<Request> {
    config: MultiProviderCallConfig,
    request: Request,
}

impl<Request> MultiProviderJsonRpcCall<Request> {
    pub fn new(config: MultiProviderCallConfig, request: Request) -> Self {
        MultiProviderJsonRpcCall { config, request }
    }
}

/// Represents a JSON-RPC HTTP call made to multiple providers whose results are then aggregated.
impl<Request, Output> MultiProviderJsonRpcCall<Request>
where
    Request: JsonRpcCall<Output>,
{
    pub async fn send_and_reduce(self) -> MultiRpcResult<Output> {
        let metrics_data = self.request.metrics_rpc_method();
        let result = self
            .parallel_call()
            .await
            .reduce(self.config.reduction_strategy);
        Request::add_inconsistent_result_metrics(metrics_data, &result);
        into_multi_rpc_result(result)
    }

    /// Query all providers in parallel and return results.
    async fn parallel_call(self) -> MultiResults<RpcService, Output, RpcError> {
        let requests = create_json_rpc_requests(self.request, self.config);

        let client = http_client(true).map_result(Request::extract_result);

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
            self.providers().len(),
            "BUG: expected 1 result per provider"
        );
        results
    }

    /// Estimate the exact cycles cost for the given request.
    ///
    /// *IMPORTANT*: the method is *synchronous* in a canister environment.
    pub async fn cycles_cost(self) -> RpcResult<u128> {
        async fn extract_request(
            request: IcHttpRequest,
        ) -> Result<HttpResponse<IcHttpRequest>, HttpClientError> {
            Ok(HttpResponse::new(request))
        }

        let requests = self.create_json_rpc_requests();

        let client = service_request_builder()
            .service_fn(extract_request)
            .map_err(RpcError::from)
            .map_response(HttpResponse::into_body);

        let (requests, errors) = requests.into_inner();
        if let Some(error) = errors.into_values().next() {
            return Err(error);
        }

        let (_client, results) = canhttp::multi::parallel_call(client, requests).await;
        let (requests, errors) = results.into_inner();
        if let Some(error) = errors.into_values().next() {
            return Err(error);
        }
        assert_eq!(
            requests.len(),
            self.providers().len(),
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
}

fn create_json_rpc_requests<T>(
    request: impl JsonRpcCall<T>,
    config: MultiProviderCallConfig,
) -> MultiResults<RpcService, HttpRequest<T>, RpcError> {
    let metrics_rpc_method = request.metrics_rpc_method();

    let transform_context = TransformContext {
        function: TransformFunc(candid::Func {
            method: "cleanup_response".to_string(),
            principal: ic_cdk::api::canister_self(),
        }),
        context: {
            let mut buf = vec![];
            minicbor::encode(config.transform, &mut buf).unwrap();
            buf
        },
    };
    let max_response_bytes = config.response_size_estimate.get();
    let body = request.into_request();

    let requests = config.providers.into_iter().map(|provider| {
        let request = resolve_rpc_service(provider.clone())
            .map_err(RpcError::from)
            .and_then(|rpc_service| rpc_service.post(&get_override_provider()))
            .map(|request_builder| {
                request_builder
                    .max_response_bytes(max_response_bytes)
                    .transform_context(transform_context.clone())
                    .body(body)
                    .expect("BUG: invalid request")
            })
            .map(|mut request_builder| {
                // Store the original `RpcService` for usage when recording metrics
                request_builder.extensions_mut().insert(provider.clone());
                // Store `MetricRpcMethod` for usage when recording metrics, which cannot simply
                // later be determined from the JSON-RPC request method since we distinguish
                // manual requests.
                request_builder
                    .extensions_mut()
                    .insert(metrics_rpc_method.clone());
                request_builder
            });
        (provider.clone(), request)
    });
    MultiResults::from_non_empty_iter(requests);
}

pub struct MultiProviderCallConfig {
    providers: BTreeSet<RpcService>,
    response_size_estimate: ResponseSizeEstimate,
    transform: ResponseTransform,
    reduction_strategy: ReductionStrategy,
}

impl MultiProviderCallConfig {
    pub fn new(
        transform: ResponseTransform,
        response_size_estimate: ResponseSizeEstimate,
        reduction_strategy: ReductionStrategy,
        providers: BTreeSet<RpcService>,
    ) -> Self {
        MultiProviderCallConfig {
            transform,
            response_size_estimate,
            reduction_strategy,
            providers,
        }
    }
}

trait JsonRpcCall<Output> {
    type Request;
    type Response;
    type MetricsRpcMethod;

    fn into_request(self) -> Self::Request;
    fn metrics_rpc_method(&self) -> Self::MetricsRpcMethod;
    fn extract_result(response: RpcResult<HttpRequest<Self::Response>>) -> RpcResult<Output>;
    fn add_inconsistent_result_metrics(
        method: Self::MetricsRpcMethod,
        result: &ReducedResult<RpcService, Output, RpcError>,
    );
}

pub struct SingleJsonRpcCall<Params, Output> {
    method: RpcMethod,
    params: Params,
    _marker: PhantomData<Output>,
}

impl<Params, Output> SingleJsonRpcCall<Params, Output> {
    pub fn new(method: RpcMethod, params: Params) -> Self {
        SingleJsonRpcCall {
            method,
            params,
            _marker: PhantomData,
        }
    }
}

impl<Params, Output> JsonRpcCall<Output> for SingleJsonRpcCall<Params, Output> {
    type Request = JsonRpcRequest<Params>;
    type Response = JsonRpcResponse<Output>;
    type MetricsRpcMethod = MetricRpcMethod;

    fn into_request(self) -> Self::Request {
        JsonRpcRequest::new(self.method.name(), self.params)
    }

    fn metrics_rpc_method(&self) -> Self::MetricsRpcMethod {
        MetricRpcMethod::from(self.method.clone())
    }

    fn extract_result(result: RpcResult<HttpRequest<Self::Response>>) -> RpcResult<Output> {
        match result?.into_body().into_result() {
            Ok(value) => Ok(value),
            Err(json_rpc_error) => Err(RpcError::JsonRpcError(JsonRpcError {
                code: json_rpc_error.code,
                message: json_rpc_error.message,
            })),
        }
    }

    fn add_inconsistent_result_metrics(
        method: Self::MetricsRpcMethod,
        result: &ReducedResult<RpcService, Output, RpcError>,
    ) {
        if let Err(ReductionError::InconsistentResults(results)) = result {
            results.iter().for_each(|(service, _)| {
                if let Ok(ResolvedRpcService::Provider(provider)) =
                    resolve_rpc_service(service.clone())
                {
                    add_metric_entry!(
                        inconsistent_responses,
                        (
                            method,
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
        }
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

fn into_multi_rpc_result<T>(result: ReducedResult<RpcService, T, RpcError>) -> MultiRpcResult<T> {
    match result {
        Ok(value) => MultiRpcResult::Consistent(Ok(value)),
        Err(err) => match err {
            ReductionError::ConsistentError(err) => MultiRpcResult::Consistent(Err(err)),
            ReductionError::InconsistentResults(results) => {
                MultiRpcResult::Inconsistent(results.into_iter().collect())
            }
        },
    }
}

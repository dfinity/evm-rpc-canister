use crate::{
    add_metric_entry,
    http::{
        charging_policy_with_collateral, error::HttpClientError, http_batch_client, http_client,
        service_request_builder,
    },
    memory::{get_override_provider, next_request_id, rank_providers, record_ok_result},
    providers::{resolve_rpc_service, SupportedRpcService},
    rpc_client::{
        eth_rpc::{
            ResponseSizeEstimate, ResponseTransform, ResponseTransformEnvelope, HEADER_SIZE_LIMIT,
            MAX_PAYLOAD_SIZE,
        },
        json::responses::RawJson,
        numeric::TransactionCount,
    },
    types::{MetricRpcMethod, MetricRpcService, ResolvedRpcService, RpcMethod},
};
use canhttp::{
    cycles::CyclesChargingPolicy,
    http::json::{
        BatchJsonRpcRequest, HttpBatchJsonRpcResponse, HttpJsonRpcResponse, Id, JsonRpcRequest,
        JsonRpcResponse,
    },
    multi::{
        MultiResults, Reduce, ReduceWithEquality, ReduceWithThreshold, ReducedResult,
        ReductionError, Timestamp,
    },
    MaxResponseBytesRequestExtension, TransformContextRequestExtension,
};
use evm_rpc_types::{
    ConsensusStrategy, JsonRpcError, MultiRpcResult, ProviderError, RpcConfig, RpcError, RpcResult,
    RpcService, RpcServices,
};
use http::{Request, Response};
use ic_cdk::management_canister::{
    HttpRequestArgs as IcHttpRequest, TransformContext, TransformFunc,
};
use json::{
    requests::{
        BlockSpec, EthCallParams, FeeHistoryParams, GetBlockByNumberParams, GetLogsParams,
        GetTransactionCountParams,
    },
    responses::{Block, Data, FeeHistory, LogEntry, SendRawTransactionResult, TransactionReceipt},
    Hash,
};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Debug,
};
use tower::ServiceExt;

pub mod amount;
pub(crate) mod eth_rpc;
mod eth_rpc_error;
pub(crate) mod json;
mod numeric;

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Default, Debug, Eq, PartialEq)]
pub struct EthereumNetwork(u64);

impl From<u64> for EthereumNetwork {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl EthereumNetwork {
    pub const MAINNET: EthereumNetwork = EthereumNetwork(1);
    pub const SEPOLIA: EthereumNetwork = EthereumNetwork(11155111);
    pub const ARBITRUM: EthereumNetwork = EthereumNetwork(42161);
    pub const BASE: EthereumNetwork = EthereumNetwork(8453);
    pub const OPTIMISM: EthereumNetwork = EthereumNetwork(10);

    pub fn chain_id(&self) -> u64 {
        self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Providers {
    chain: EthereumNetwork,
    /// *Non-empty* set of providers to query.
    services: BTreeSet<RpcService>,
}

impl Providers {
    const DEFAULT_NUM_PROVIDERS_FOR_EQUALITY: usize = 3;

    pub fn new(
        source: RpcServices,
        strategy: ConsensusStrategy,
        now: Timestamp,
    ) -> Result<Self, ProviderError> {
        fn user_defined_providers(source: RpcServices) -> Option<Vec<RpcService>> {
            fn map_services<T, F>(
                services: impl Into<Option<Vec<T>>>,
                f: F,
            ) -> Option<Vec<RpcService>>
            where
                F: Fn(T) -> RpcService,
            {
                services.into().map(|s| s.into_iter().map(f).collect())
            }
            match source {
                RpcServices::Custom { services, .. } => map_services(services, RpcService::Custom),
                RpcServices::EthMainnet(services) => map_services(services, RpcService::EthMainnet),
                RpcServices::EthSepolia(services) => map_services(services, RpcService::EthSepolia),
                RpcServices::ArbitrumOne(services) => {
                    map_services(services, RpcService::ArbitrumOne)
                }
                RpcServices::BaseMainnet(services) => {
                    map_services(services, RpcService::BaseMainnet)
                }
                RpcServices::OptimismMainnet(services) => {
                    map_services(services, RpcService::OptimismMainnet)
                }
            }
        }

        fn supported_providers(
            source: &RpcServices,
        ) -> (EthereumNetwork, &'static [SupportedRpcService]) {
            match source {
                RpcServices::Custom { chain_id, .. } => (EthereumNetwork::from(*chain_id), &[]),
                RpcServices::EthMainnet(_) => {
                    (EthereumNetwork::MAINNET, SupportedRpcService::eth_mainnet())
                }
                RpcServices::EthSepolia(_) => {
                    (EthereumNetwork::SEPOLIA, SupportedRpcService::eth_sepolia())
                }
                RpcServices::ArbitrumOne(_) => (
                    EthereumNetwork::ARBITRUM,
                    SupportedRpcService::arbitrum_one(),
                ),
                RpcServices::BaseMainnet(_) => {
                    (EthereumNetwork::BASE, SupportedRpcService::base_mainnet())
                }
                RpcServices::OptimismMainnet(_) => (
                    EthereumNetwork::OPTIMISM,
                    SupportedRpcService::optimism_mainnet(),
                ),
            }
        }

        let (chain, supported_providers) = supported_providers(&source);
        let user_input = user_defined_providers(source);
        let providers = choose_providers(user_input, supported_providers, strategy, now)?;

        if providers.is_empty() {
            return Err(ProviderError::ProviderNotFound);
        }

        Ok(Self {
            chain,
            services: providers,
        })
    }
}

fn choose_providers(
    user_input: Option<Vec<RpcService>>,
    supported_providers: &[SupportedRpcService],
    strategy: ConsensusStrategy,
    now: Timestamp,
) -> Result<BTreeSet<RpcService>, ProviderError> {
    match strategy {
        ConsensusStrategy::Equality => Ok(user_input
            .unwrap_or_else(|| {
                rank_providers(supported_providers, now)
                    .into_iter()
                    .take(Providers::DEFAULT_NUM_PROVIDERS_FOR_EQUALITY)
                    .map(RpcService::from)
                    .collect()
            })
            .into_iter()
            .collect()),
        ConsensusStrategy::Threshold { total, min } => {
            // Ensure that
            // 0 < min <= total <= all_providers.len()
            if min == 0 {
                return Err(ProviderError::InvalidRpcConfig(
                    "min must be greater than 0".to_string(),
                ));
            }
            match user_input {
                None => {
                    let total = total.ok_or_else(|| {
                        ProviderError::InvalidRpcConfig(
                            "total must be specified when using default providers".to_string(),
                        )
                    })?;

                    if min > total {
                        return Err(ProviderError::InvalidRpcConfig(format!(
                            "min {} is greater than total {}",
                            min, total
                        )));
                    }

                    let all_providers_len = supported_providers.len();
                    if total > all_providers_len as u8 {
                        return Err(ProviderError::InvalidRpcConfig(format!(
                            "total {} is greater than the number of all supported providers {}",
                            total, all_providers_len
                        )));
                    }
                    let providers: BTreeSet<_> = rank_providers(supported_providers, now)
                        .into_iter()
                        .take(total as usize)
                        .map(RpcService::from)
                        .collect();
                    assert_eq!(providers.len(), total as usize, "BUG: duplicate providers");
                    Ok(providers)
                }
                Some(providers) => {
                    if min > providers.len() as u8 {
                        return Err(ProviderError::InvalidRpcConfig(format!(
                            "min {} is greater than the number of specified providers {}",
                            min,
                            providers.len()
                        )));
                    }
                    if let Some(total) = total {
                        if total != providers.len() as u8 {
                            return Err(ProviderError::InvalidRpcConfig(format!(
                                "total {} is different than the number of specified providers {}",
                                total,
                                providers.len()
                            )));
                        }
                    }
                    Ok(providers.into_iter().collect())
                }
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EthRpcClient {
    providers: Providers,
    config: RpcConfig,
}

impl EthRpcClient {
    pub fn new(
        source: RpcServices,
        config: Option<RpcConfig>,
        now: Timestamp,
    ) -> Result<Self, ProviderError> {
        let config = config.unwrap_or_default();
        let strategy = config.response_consensus.clone().unwrap_or_default();
        Ok(Self {
            providers: Providers::new(source, strategy, now)?,
            config,
        })
    }

    fn chain(&self) -> EthereumNetwork {
        self.providers.chain
    }

    fn response_size_estimate(&self, estimate: u64) -> ResponseSizeEstimate {
        ResponseSizeEstimate::new(self.config.response_size_estimate.unwrap_or(estimate))
    }

    fn reduction_strategy(&self) -> ReductionStrategy {
        ReductionStrategy::from(
            self.config
                .response_consensus
                .as_ref()
                .cloned()
                .unwrap_or_default(),
        )
    }

    fn https_outcall_settings(
        &self,
        method: &RpcMethod,
    ) -> (ResponseSizeEstimate, ResponseTransform) {
        match method {
            RpcMethod::EthCall => (
                self.response_size_estimate(256 + HEADER_SIZE_LIMIT),
                ResponseTransform::Call,
            ),
            RpcMethod::EthFeeHistory => (
                // A typical response is slightly above 300 bytes.
                self.response_size_estimate(512 + HEADER_SIZE_LIMIT),
                ResponseTransform::FeeHistory,
            ),
            RpcMethod::EthGetLogs => (
                self.response_size_estimate(1024 + HEADER_SIZE_LIMIT),
                ResponseTransform::GetLogs,
            ),
            RpcMethod::EthGetBlockByNumber => {
                let expected_block_size = match self.chain() {
                    EthereumNetwork::SEPOLIA => 12 * 1024,
                    EthereumNetwork::MAINNET => 24 * 1024,
                    _ => 24 * 1024, // Default for unknown networks
                };
                (
                    self.response_size_estimate(expected_block_size + HEADER_SIZE_LIMIT),
                    ResponseTransform::GetBlockByNumber,
                )
            }
            RpcMethod::EthGetTransactionCount => (
                self.response_size_estimate(50 + HEADER_SIZE_LIMIT),
                ResponseTransform::GetTransactionCount,
            ),
            RpcMethod::EthGetTransactionReceipt => (
                self.response_size_estimate(700 + HEADER_SIZE_LIMIT),
                ResponseTransform::GetTransactionReceipt,
            ),
            RpcMethod::EthSendRawTransaction => (
                // A successful reply is under 256 bytes, but we expect most calls to end with an error
                // since we submit the same transaction from multiple nodes.
                self.response_size_estimate(256 + HEADER_SIZE_LIMIT),
                ResponseTransform::SendRawTransaction,
            ),
            RpcMethod::Custom(_) => (
                self.response_size_estimate(256 + HEADER_SIZE_LIMIT),
                ResponseTransform::Raw,
            ),
        }
    }

    pub fn eth_get_logs(
        self,
        params: GetLogsParams,
    ) -> MultiRpcSingleRequest<(GetLogsParams,), Vec<LogEntry>> {
        self.single_request(RpcMethod::EthGetLogs, (params,))
    }

    pub fn eth_get_block_by_number(
        self,
        block: BlockSpec,
    ) -> MultiRpcSingleRequest<GetBlockByNumberParams, Block> {
        self.single_request(
            RpcMethod::EthGetBlockByNumber,
            GetBlockByNumberParams {
                block,
                include_full_transactions: false,
            },
        )
    }

    pub fn eth_get_transaction_receipt(
        self,
        tx_hash: Hash,
    ) -> MultiRpcSingleRequest<(Hash,), Option<TransactionReceipt>> {
        self.single_request(RpcMethod::EthGetTransactionReceipt, (tx_hash,))
    }

    pub fn eth_fee_history(
        self,
        params: FeeHistoryParams,
    ) -> MultiRpcSingleRequest<FeeHistoryParams, FeeHistory> {
        self.single_request(RpcMethod::EthFeeHistory, params)
    }

    pub fn eth_send_raw_transaction(
        self,
        raw_signed_transaction_hex: String,
    ) -> MultiRpcSingleRequest<(String,), SendRawTransactionResult> {
        self.single_request(
            RpcMethod::EthSendRawTransaction,
            (raw_signed_transaction_hex,),
        )
    }

    pub fn eth_get_transaction_count(
        self,
        params: GetTransactionCountParams,
    ) -> MultiRpcSingleRequest<GetTransactionCountParams, TransactionCount> {
        self.single_request(RpcMethod::EthGetTransactionCount, params)
    }

    pub fn eth_call(self, params: EthCallParams) -> MultiRpcSingleRequest<EthCallParams, Data> {
        self.single_request(RpcMethod::EthCall, params)
    }

    pub fn eth_batch(self, requests: Vec<BatchRequestItem>) -> MultiRpcRequest<BatchPayload> {
        let total_estimate: u64 = requests
            .iter()
            .map(|r| r.response_size_estimate)
            .sum::<u64>()
            .min(MAX_PAYLOAD_SIZE);
        let response_size_estimate =
            ResponseSizeEstimate::new((total_estimate + HEADER_SIZE_LIMIT).min(MAX_PAYLOAD_SIZE));
        let reduction_strategy = self.reduction_strategy();
        MultiRpcRequest {
            providers: self.providers.services,
            payload: BatchPayload {
                requests,
                response_size_estimate,
            },
            reduction_strategy,
        }
    }

    pub fn multi_request(
        self,
        method: RpcMethod,
        params: Option<&Value>,
    ) -> MultiRpcSingleRequest<Option<&Value>, RawJson> {
        self.single_request(method, params)
    }

    fn single_request<Params, Output>(
        self,
        method: RpcMethod,
        params: Params,
    ) -> MultiRpcSingleRequest<Params, Output> {
        let (response_size_estimate, transform) = self.https_outcall_settings(&method);
        let reduction_strategy = self.reduction_strategy();
        MultiRpcRequest::new(
            self.providers.services,
            method,
            params,
            response_size_estimate,
            transform,
            reduction_strategy,
        )
    }
}

/// Abstraction over the payload of a multi-provider request.
/// Implemented by [`SinglePayload`] for single JSON-RPC methods
/// and [`BatchPayload`] for batch requests.
pub trait RequestPayload {
    type HttpBody: Serialize;

    fn response_size_estimate(&self) -> u64;

    fn create_http_requests(
        &self,
        providers: &BTreeSet<RpcService>,
    ) -> MultiResults<RpcService, Request<Self::HttpBody>, RpcError>;
}

pub struct SinglePayload<Params, Output> {
    method: RpcMethod,
    params: Params,
    response_size_estimate: ResponseSizeEstimate,
    transform: ResponseTransformEnvelope,
    _marker: std::marker::PhantomData<Output>,
}

impl<Params: Serialize + Clone, Output> RequestPayload for SinglePayload<Params, Output> {
    type HttpBody = JsonRpcRequest<Params>;

    fn response_size_estimate(&self) -> u64 {
        self.response_size_estimate.get()
    }

    fn create_http_requests(
        &self,
        providers: &BTreeSet<RpcService>,
    ) -> MultiResults<RpcService, Request<Self::HttpBody>, RpcError> {
        let response_size_estimate = self.response_size_estimate();
        let transform_op = {
            let mut buf = vec![];
            minicbor::encode(&self.transform, &mut buf).unwrap();
            buf
        };
        let mut requests = MultiResults::default();
        for provider in providers.iter() {
            let request = resolve_rpc_service(provider.clone())
                .map_err(RpcError::from)
                .and_then(|rpc_service| rpc_service.post(&get_override_provider()))
                .map(|builder| {
                    builder
                        .max_response_bytes(response_size_estimate)
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
                    request.extensions_mut().insert(provider.clone());
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

/// A single item in a batch request, containing the pre-serialized JSON-RPC
/// parameters and the metadata needed for response transformation and size estimation.
pub struct BatchRequestItem {
    pub method: RpcMethod,
    pub params: Value,
    pub transform: ResponseTransform,
    pub response_size_estimate: u64,
}

pub struct BatchPayload {
    requests: Vec<BatchRequestItem>,
    response_size_estimate: ResponseSizeEstimate,
}

impl RequestPayload for BatchPayload {
    type HttpBody = BatchJsonRpcRequest<Value>;

    fn response_size_estimate(&self) -> u64 {
        self.response_size_estimate.get()
    }

    fn create_http_requests(
        &self,
        providers: &BTreeSet<RpcService>,
    ) -> MultiResults<RpcService, Request<Self::HttpBody>, RpcError> {
        let response_size_estimate = self.response_size_estimate();
        let mut requests = MultiResults::default();
        for provider in providers.iter() {
            let ids: Vec<Id> = (0..self.requests.len())
                .map(|_| next_request_id())
                .collect();

            let transform_envelope = ResponseTransformEnvelope::from(
                ids.iter()
                    .zip(self.requests.iter())
                    .map(|(id, item)| (id.clone(), item.transform.clone()))
                    .collect::<BTreeMap<_, _>>(),
            );

            let transform_op = {
                let mut buf = vec![];
                minicbor::encode(&transform_envelope, &mut buf).unwrap();
                buf
            };

            let batch_body: BatchJsonRpcRequest<Value> = ids
                .iter()
                .zip(self.requests.iter())
                .map(|(id, item)| {
                    JsonRpcRequest::new(item.method.clone().name(), item.params.clone())
                        .with_id(id.clone())
                })
                .collect();

            let request = resolve_rpc_service(provider.clone())
                .map_err(RpcError::from)
                .and_then(|rpc_service| rpc_service.post(&get_override_provider()))
                .map(|builder| {
                    builder
                        .max_response_bytes(response_size_estimate)
                        .transform_context(TransformContext {
                            function: TransformFunc(candid::Func {
                                method: "cleanup_response".to_string(),
                                principal: ic_cdk::api::canister_self(),
                            }),
                            context: transform_op,
                        })
                        .body(batch_body)
                        .expect("BUG: invalid request")
                })
                .map(|mut request| {
                    request.extensions_mut().insert(provider.clone());
                    request
                        .extensions_mut()
                        .insert(MetricRpcMethod::from(RpcMethod::Custom(
                            "eth_batch".to_string(),
                        )));
                    request
                });
            requests.insert_once(provider.clone(), request);
        }
        requests
    }
}

/// A single RPC request made to multiple providers.
pub type MultiRpcSingleRequest<Params, Output> = MultiRpcRequest<SinglePayload<Params, Output>>;

/// An RPC request made to a set of providers.
pub struct MultiRpcRequest<Payload> {
    providers: BTreeSet<RpcService>,
    payload: Payload,
    reduction_strategy: ReductionStrategy,
}

impl<P: RequestPayload> MultiRpcRequest<P> {
    /// Estimate the exact cycles cost for the given request.
    ///
    /// *IMPORTANT*: the method is *synchronous* in a canister environment.
    pub async fn cycles_cost(&self) -> RpcResult<u128> {
        async fn extract_request(
            request: IcHttpRequest,
        ) -> Result<Response<IcHttpRequest>, HttpClientError> {
            Ok(Response::new(request))
        }

        let requests = self
            .payload
            .create_http_requests(&self.providers);

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
}

impl<Params, Output> MultiRpcSingleRequest<Params, Output> {
    pub fn new(
        providers: BTreeSet<RpcService>,
        method: RpcMethod,
        params: Params,
        response_size_estimate: ResponseSizeEstimate,
        transform: impl Into<ResponseTransformEnvelope>,
        reduction_strategy: ReductionStrategy,
    ) -> Self {
        MultiRpcRequest {
            providers,
            payload: SinglePayload {
                method,
                params,
                response_size_estimate,
                transform: transform.into(),
                _marker: Default::default(),
            },
            reduction_strategy,
        }
    }

    pub async fn send_and_reduce(self) -> MultiRpcResult<Output>
    where
        Params: Serialize + Clone + Debug,
        Output: Debug + Serialize + DeserializeOwned + PartialEq,
    {
        let result = self.parallel_call().await.reduce(self.reduction_strategy);
        process_result(self.payload.method, result)
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
        let requests = self
            .payload
            .create_http_requests(&self.providers);

        let client = http_client(true).map_result(extract_json_rpc_response);

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
}

impl MultiRpcRequest<BatchPayload> {
    pub async fn send_and_reduce(self) -> Vec<MultiRpcResult<JsonRpcResponse<Value>>> {
        let batch_size = self.payload.requests.len();
        let multi_results = self.parallel_call().await;

        // Transpose: from per-provider Vec<Response> to per-position MultiResults<Response>
        let mut per_position: Vec<MultiResults<RpcService, JsonRpcResponse<Value>, RpcError>> =
            (0..batch_size).map(|_| MultiResults::default()).collect();

        let (ok_results, errors) = multi_results.into_inner();
        for (service, responses) in ok_results {
            for (i, response) in responses.into_iter().enumerate() {
                per_position[i].insert_once(service.clone(), Ok(response));
            }
        }
        for (service, error) in errors {
            for pos in per_position.iter_mut() {
                pos.insert_once(service.clone(), Err(error.clone()));
            }
        }

        let method = RpcMethod::Custom("eth_batch".to_string());
        per_position
            .into_iter()
            .map(|results| {
                let result = results.reduce(self.reduction_strategy.clone());
                process_result(method.clone(), result)
            })
            .collect()
    }

    async fn parallel_call(
        &self,
    ) -> MultiResults<RpcService, Vec<JsonRpcResponse<Value>>, RpcError> {
        let requests = self
            .payload
            .create_http_requests(&self.providers);

        let client = http_batch_client().map_result(extract_batch_json_rpc_response);

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
}

fn extract_batch_json_rpc_response(
    result: RpcResult<HttpBatchJsonRpcResponse<Value>>,
) -> RpcResult<Vec<JsonRpcResponse<Value>>> {
    Ok(result?.into_body())
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

#[derive(Clone)]
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

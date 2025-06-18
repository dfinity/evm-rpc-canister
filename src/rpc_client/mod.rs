use crate::http::http_client;
use crate::memory::{get_override_provider, record_ok_result};
use crate::providers::{resolve_rpc_service, SupportedRpcService};
use crate::rpc_client::eth_rpc::{HttpResponsePayload, ResponseSizeEstimate, HEADER_SIZE_LIMIT};
use crate::rpc_client::numeric::TransactionCount;
use crate::types::MetricRpcMethod;
use canhttp::{
    http::json::JsonRpcRequest,
    multi::{MultiResults, Reduce, ReduceWithEquality, ReduceWithThreshold},
    MaxResponseBytesRequestExtension, TransformContextRequestExtension,
};
use evm_rpc_types::{
    ConsensusStrategy, JsonRpcError, ProviderError, RpcConfig, RpcError, RpcService, RpcServices,
};
use ic_cdk::api::management_canister::http_request::TransformContext;
use json::requests::{
    BlockSpec, EthCallParams, FeeHistoryParams, GetBlockByNumberParams, GetLogsParam,
    GetTransactionCountParams,
};
use json::responses::{
    Block, Data, FeeHistory, LogEntry, SendRawTransactionResult, TransactionReceipt,
};
use json::Hash;
use serde::{de::DeserializeOwned, Serialize};
use std::collections::BTreeSet;
use std::fmt::Debug;
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

    pub fn new(source: RpcServices, strategy: ConsensusStrategy) -> Result<Self, ProviderError> {
        fn user_defined_providers(source: RpcServices) -> Option<Vec<RpcService>> {
            match source {
                RpcServices::Custom { services, .. } => {
                    Some(services.into_iter().map(RpcService::Custom).collect())
                }
                RpcServices::EthMainnet(services) => {
                    services.map(|s| s.into_iter().map(RpcService::EthMainnet).collect())
                }
                RpcServices::EthSepolia(services) => {
                    services.map(|s| s.into_iter().map(RpcService::EthSepolia).collect())
                }
                RpcServices::ArbitrumOne(services) => {
                    services.map(|s| s.into_iter().map(RpcService::ArbitrumOne).collect())
                }
                RpcServices::BaseMainnet(services) => {
                    services.map(|s| s.into_iter().map(RpcService::BaseMainnet).collect())
                }
                RpcServices::OptimismMainnet(services) => {
                    services.map(|s| s.into_iter().map(RpcService::OptimismMainnet).collect())
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
        let providers = choose_providers(user_input, supported_providers, strategy)?;

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
) -> Result<BTreeSet<RpcService>, ProviderError> {
    match strategy {
        ConsensusStrategy::Equality => Ok(user_input
            .unwrap_or_else(|| {
                supported_providers
                    .iter()
                    .take(Providers::DEFAULT_NUM_PROVIDERS_FOR_EQUALITY)
                    .copied()
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
                    let providers: BTreeSet<_> = supported_providers
                        .iter()
                        .take(total as usize)
                        .copied()
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
    pub fn new(source: RpcServices, config: Option<RpcConfig>) -> Result<Self, ProviderError> {
        let config = config.unwrap_or_default();
        let strategy = config.response_consensus.clone().unwrap_or_default();
        Ok(Self {
            providers: Providers::new(source, strategy)?,
            config,
        })
    }

    fn chain(&self) -> EthereumNetwork {
        self.providers.chain
    }

    fn providers(&self) -> &BTreeSet<RpcService> {
        &self.providers.services
    }

    fn response_size_estimate(&self, estimate: u64) -> ResponseSizeEstimate {
        ResponseSizeEstimate::new(self.config.response_size_estimate.unwrap_or(estimate))
    }

    fn consensus_strategy(&self) -> ReductionStrategy {
        ReductionStrategy::from(
            self.config
                .response_consensus
                .as_ref()
                .cloned()
                .unwrap_or_default(),
        )
    }

    /// Query all providers in parallel and return all results.
    /// It's up to the caller to decide how to handle the results, which could be inconsistent
    /// (e.g., if different providers gave different responses).
    /// This method is useful for querying data that is critical for the system to ensure that there is no single point of failure,
    /// e.g., ethereum logs upon which ckETH will be minted.
    async fn parallel_call<I, O>(
        &self,
        method: impl Into<String> + Clone,
        params: I,
        response_size_estimate: ResponseSizeEstimate,
    ) -> MultiCallResults<O>
    where
        I: Serialize + Clone + Debug,
        O: Debug + DeserializeOwned + HttpResponsePayload,
    {
        let providers = self.providers();
        let transform_op = O::response_transform()
            .as_ref()
            .map(|t| {
                let mut buf = vec![];
                minicbor::encode(t, &mut buf).unwrap();
                buf
            })
            .unwrap_or_default();
        let effective_size_estimate = response_size_estimate.get();
        let mut requests = MultiResults::default();
        for provider in providers {
            let request = resolve_rpc_service(provider.clone())
                .map_err(RpcError::from)
                .and_then(|rpc_service| rpc_service.post(&get_override_provider()))
                .map(|builder| {
                    builder
                        .max_response_bytes(effective_size_estimate)
                        .transform_context(TransformContext::from_name(
                            "cleanup_response".to_owned(),
                            transform_op.clone(),
                        ))
                        .body(JsonRpcRequest::new(method.clone(), params.clone()))
                        .expect("BUG: invalid request")
                });
            requests.insert_once(provider.clone(), request);
        }

        let client = http_client(MetricRpcMethod(method.into()), true).map_result(|r| {
            match r?.into_body().into_result() {
                Ok(value) => Ok(value),
                Err(json_rpc_error) => Err(RpcError::JsonRpcError(JsonRpcError {
                    code: json_rpc_error.code,
                    message: json_rpc_error.message,
                })),
            }
        });

        let (requests, errors) = requests.into_inner();
        let (_client, mut results) = canhttp::multi::parallel_call(client, requests).await;
        results.add_errors(errors);
        results
            .ok_results()
            .keys()
            .for_each(|service| record_ok_result(service));
        assert_eq!(
            results.len(),
            providers.len(),
            "BUG: expected 1 result per provider"
        );
        results
    }

    pub async fn eth_get_logs(&self, params: GetLogsParam) -> ReducedResult<Vec<LogEntry>> {
        self.parallel_call(
            "eth_getLogs",
            vec![params],
            self.response_size_estimate(1024 + HEADER_SIZE_LIMIT),
        )
        .await
        .reduce(self.consensus_strategy())
    }

    pub async fn eth_get_block_by_number(&self, block: BlockSpec) -> ReducedResult<Block> {
        let expected_block_size = match self.chain() {
            EthereumNetwork::SEPOLIA => 12 * 1024,
            EthereumNetwork::MAINNET => 24 * 1024,
            _ => 24 * 1024, // Default for unknown networks
        };

        self.parallel_call(
            "eth_getBlockByNumber",
            GetBlockByNumberParams {
                block,
                include_full_transactions: false,
            },
            self.response_size_estimate(expected_block_size + HEADER_SIZE_LIMIT),
        )
        .await
        .reduce(self.consensus_strategy())
    }

    pub async fn eth_get_transaction_receipt(
        &self,
        tx_hash: Hash,
    ) -> ReducedResult<Option<TransactionReceipt>> {
        self.parallel_call(
            "eth_getTransactionReceipt",
            vec![tx_hash],
            self.response_size_estimate(700 + HEADER_SIZE_LIMIT),
        )
        .await
        .reduce(self.consensus_strategy())
    }

    pub async fn eth_fee_history(&self, params: FeeHistoryParams) -> ReducedResult<FeeHistory> {
        // A typical response is slightly above 300 bytes.
        self.parallel_call(
            "eth_feeHistory",
            params,
            self.response_size_estimate(512 + HEADER_SIZE_LIMIT),
        )
        .await
        .reduce(self.consensus_strategy())
    }

    pub async fn eth_send_raw_transaction(
        &self,
        raw_signed_transaction_hex: String,
    ) -> ReducedResult<SendRawTransactionResult> {
        // A successful reply is under 256 bytes, but we expect most calls to end with an error
        // since we submit the same transaction from multiple nodes.
        self.parallel_call(
            "eth_sendRawTransaction",
            vec![raw_signed_transaction_hex],
            self.response_size_estimate(256 + HEADER_SIZE_LIMIT),
        )
        .await
        .reduce(self.consensus_strategy())
    }

    pub async fn eth_get_transaction_count(
        &self,
        params: GetTransactionCountParams,
    ) -> ReducedResult<TransactionCount> {
        self.parallel_call(
            "eth_getTransactionCount",
            params,
            self.response_size_estimate(50 + HEADER_SIZE_LIMIT),
        )
        .await
        .reduce(self.consensus_strategy())
    }

    pub async fn eth_call(&self, params: EthCallParams) -> ReducedResult<Data> {
        self.parallel_call(
            "eth_call",
            params,
            self.response_size_estimate(256 + HEADER_SIZE_LIMIT),
        )
        .await
        .reduce(self.consensus_strategy())
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
    fn reduce(&self, results: MultiResults<RpcService, T, RpcError>) -> ReducedResult<T> {
        match self {
            ReductionStrategy::ByEquality(r) => r.reduce(results),
            ReductionStrategy::ByThreshold(r) => r.reduce(results),
        }
    }
}

pub type MultiCallResults<T> = MultiResults<RpcService, T, RpcError>;
pub type ReducedResult<T> = canhttp::multi::ReducedResult<RpcService, T, RpcError>;

use crate::http::http_client;
use crate::memory::get_override_provider;
use crate::providers::resolve_rpc_service;
use crate::rpc_client::eth_rpc::{HttpResponsePayload, ResponseSizeEstimate, HEADER_SIZE_LIMIT};
use crate::rpc_client::numeric::TransactionCount;
use crate::types::MetricRpcMethod;
use canhttp::{
    http::json::JsonRpcRequest,
    multi::{MultiResults, Reduce, ReduceWithEquality, ReduceWithThreshold},
    MaxResponseBytesRequestExtension, TransformContextRequestExtension,
};
use evm_rpc_types::{
    ConsensusStrategy, EthMainnetService, EthSepoliaService, JsonRpcError, L2MainnetService,
    ProviderError, RpcConfig, RpcError, RpcService, RpcServices,
};
use ic_cdk::management_canister::transform_context_from_query;
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
    // Order of providers matters!
    // The threshold consensus strategy will consider the first `total` providers in the order
    // they are specified (taking the default ones first, followed by the non default ones if necessary)
    // if the providers are not explicitly specified by the caller.
    const DEFAULT_ETH_MAINNET_SERVICES: &'static [EthMainnetService] = &[
        EthMainnetService::BlockPi,
        EthMainnetService::Cloudflare,
        EthMainnetService::PublicNode,
    ];
    const NON_DEFAULT_ETH_MAINNET_SERVICES: &'static [EthMainnetService] = &[
        EthMainnetService::Llama,
        EthMainnetService::Alchemy,
        EthMainnetService::Ankr,
    ];

    const DEFAULT_ETH_SEPOLIA_SERVICES: &'static [EthSepoliaService] = &[
        EthSepoliaService::Ankr,
        EthSepoliaService::BlockPi,
        EthSepoliaService::PublicNode,
    ];
    const NON_DEFAULT_ETH_SEPOLIA_SERVICES: &'static [EthSepoliaService] =
        &[EthSepoliaService::Alchemy, EthSepoliaService::Sepolia];

    const DEFAULT_L2_MAINNET_SERVICES: &'static [L2MainnetService] = &[
        L2MainnetService::Llama,
        L2MainnetService::BlockPi,
        L2MainnetService::PublicNode,
    ];
    const NON_DEFAULT_L2_MAINNET_SERVICES: &'static [L2MainnetService] =
        &[L2MainnetService::Alchemy, L2MainnetService::Ankr];

    pub fn new(source: RpcServices, strategy: ConsensusStrategy) -> Result<Self, ProviderError> {
        let (chain, providers): (_, BTreeSet<_>) = match source {
            RpcServices::Custom { chain_id, services } => (
                EthereumNetwork::from(chain_id),
                choose_providers(Some(services), &[], &[], strategy)?
                    .into_iter()
                    .map(RpcService::Custom)
                    .collect(),
            ),
            RpcServices::EthMainnet(services) => (
                EthereumNetwork::MAINNET,
                choose_providers(
                    services,
                    Self::DEFAULT_ETH_MAINNET_SERVICES,
                    Self::NON_DEFAULT_ETH_MAINNET_SERVICES,
                    strategy,
                )?
                .into_iter()
                .map(RpcService::EthMainnet)
                .collect(),
            ),
            RpcServices::EthSepolia(services) => (
                EthereumNetwork::SEPOLIA,
                choose_providers(
                    services,
                    Self::DEFAULT_ETH_SEPOLIA_SERVICES,
                    Self::NON_DEFAULT_ETH_SEPOLIA_SERVICES,
                    strategy,
                )?
                .into_iter()
                .map(RpcService::EthSepolia)
                .collect(),
            ),
            RpcServices::ArbitrumOne(services) => (
                EthereumNetwork::ARBITRUM,
                choose_providers(
                    services,
                    Self::DEFAULT_L2_MAINNET_SERVICES,
                    Self::NON_DEFAULT_L2_MAINNET_SERVICES,
                    strategy,
                )?
                .into_iter()
                .map(RpcService::ArbitrumOne)
                .collect(),
            ),
            RpcServices::BaseMainnet(services) => (
                EthereumNetwork::BASE,
                choose_providers(
                    services,
                    Self::DEFAULT_L2_MAINNET_SERVICES,
                    Self::NON_DEFAULT_L2_MAINNET_SERVICES,
                    strategy,
                )?
                .into_iter()
                .map(RpcService::BaseMainnet)
                .collect(),
            ),
            RpcServices::OptimismMainnet(services) => (
                EthereumNetwork::OPTIMISM,
                choose_providers(
                    services,
                    Self::DEFAULT_L2_MAINNET_SERVICES,
                    Self::NON_DEFAULT_L2_MAINNET_SERVICES,
                    strategy,
                )?
                .into_iter()
                .map(RpcService::OptimismMainnet)
                .collect(),
            ),
        };

        if providers.is_empty() {
            return Err(ProviderError::ProviderNotFound);
        }

        Ok(Self {
            chain,
            services: providers,
        })
    }
}

fn choose_providers<T>(
    user_input: Option<Vec<T>>,
    default_providers: &[T],
    non_default_providers: &[T],
    strategy: ConsensusStrategy,
) -> Result<BTreeSet<T>, ProviderError>
where
    T: Clone + Ord,
{
    match strategy {
        ConsensusStrategy::Equality => Ok(user_input
            .unwrap_or_else(|| default_providers.to_vec())
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
                    let all_providers_len = default_providers.len() + non_default_providers.len();
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

                    if total > all_providers_len as u8 {
                        return Err(ProviderError::InvalidRpcConfig(format!(
                            "total {} is greater than the number of all supported providers {}",
                            total, all_providers_len
                        )));
                    }
                    let providers: BTreeSet<_> = default_providers
                        .iter()
                        .chain(non_default_providers.iter())
                        .take(total as usize)
                        .cloned()
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
                        .transform_context(transform_context_from_query(
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

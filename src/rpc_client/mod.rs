use crate::{memory::rank_providers, providers::SupportedRpcService, types::RpcMethod};
use canhttp::multi::Timestamp;
use eth_rpc::{ResponseSizeEstimate, ResponseTransform, HEADER_SIZE_LIMIT};
use evm_rpc_types::{ConsensusStrategy, ProviderError, RpcConfig, RpcService, RpcServices};
use ic_cdk::management_canister::HttpRequestArgs as IcHttpRequest;
use json::responses::RawJson;
use json::{
    requests::{
        BlockSpec, EthCallParams, FeeHistoryParams, GetBlockByNumberParams, GetLogsParams,
        GetTransactionCountParams,
    },
    responses::{Block, Data, FeeHistory, LogEntry, SendRawTransactionResult, TransactionReceipt},
    Hash,
};
use numeric::TransactionCount;
use request::{MultiRpcRequest, ReductionStrategy};
use serde_json::Value;
use std::{collections::BTreeSet, fmt::Debug};

pub mod amount;
pub(crate) mod eth_rpc;
mod eth_rpc_error;
pub(crate) mod json;
mod numeric;
mod request;

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

    pub fn eth_get_logs(
        self,
        params: GetLogsParams,
    ) -> MultiRpcRequest<(GetLogsParams,), Vec<LogEntry>> {
        let response_size_estimate = self.response_size_estimate(1024 + HEADER_SIZE_LIMIT);
        let reduction = self.reduction_strategy();
        MultiRpcRequest::new(
            self.providers.services,
            RpcMethod::EthGetLogs,
            (params,),
            response_size_estimate,
            ResponseTransform::GetLogs,
            reduction,
        )
    }

    pub fn eth_get_block_by_number(
        self,
        block: BlockSpec,
    ) -> MultiRpcRequest<GetBlockByNumberParams, Block> {
        let expected_block_size = match self.chain() {
            EthereumNetwork::SEPOLIA => 12 * 1024,
            EthereumNetwork::MAINNET => 24 * 1024,
            _ => 24 * 1024, // Default for unknown networks
        };
        let response_size_estimate =
            self.response_size_estimate(expected_block_size + HEADER_SIZE_LIMIT);
        let reduction_strategy = self.reduction_strategy();
        MultiRpcRequest::new(
            self.providers.services,
            RpcMethod::EthGetBlockByNumber,
            GetBlockByNumberParams {
                block,
                include_full_transactions: false,
            },
            response_size_estimate,
            ResponseTransform::GetBlockByNumber,
            reduction_strategy,
        )
    }

    pub fn eth_get_transaction_receipt(
        self,
        tx_hash: Hash,
    ) -> MultiRpcRequest<(Hash,), Option<TransactionReceipt>> {
        let response_size_estimate = self.response_size_estimate(700 + HEADER_SIZE_LIMIT);
        let reduction_strategy = self.reduction_strategy();
        MultiRpcRequest::new(
            self.providers.services,
            RpcMethod::EthGetTransactionReceipt,
            (tx_hash,),
            response_size_estimate,
            ResponseTransform::GetTransactionReceipt,
            reduction_strategy,
        )
    }

    pub fn eth_fee_history(
        self,
        params: FeeHistoryParams,
    ) -> MultiRpcRequest<FeeHistoryParams, FeeHistory> {
        // A typical response is slightly above 300 bytes.
        let response_size_estimate = self.response_size_estimate(512 + HEADER_SIZE_LIMIT);
        let reduction_strategy = self.reduction_strategy();
        MultiRpcRequest::new(
            self.providers.services,
            RpcMethod::EthFeeHistory,
            params,
            response_size_estimate,
            ResponseTransform::FeeHistory,
            reduction_strategy,
        )
    }

    pub fn eth_send_raw_transaction(
        self,
        raw_signed_transaction_hex: String,
    ) -> MultiRpcRequest<(String,), SendRawTransactionResult> {
        // A successful reply is under 256 bytes, but we expect most calls to end with an error
        // since we submit the same transaction from multiple nodes.
        let response_size_estimate = self.response_size_estimate(256 + HEADER_SIZE_LIMIT);
        let reduction_strategy = self.reduction_strategy();
        MultiRpcRequest::new(
            self.providers.services,
            RpcMethod::EthSendRawTransaction,
            (raw_signed_transaction_hex,),
            response_size_estimate,
            ResponseTransform::SendRawTransaction,
            reduction_strategy,
        )
    }

    pub fn eth_get_transaction_count(
        self,
        params: GetTransactionCountParams,
    ) -> MultiRpcRequest<GetTransactionCountParams, TransactionCount> {
        let response_size_estimate = self.response_size_estimate(50 + HEADER_SIZE_LIMIT);
        let reduction_strategy = self.reduction_strategy();
        MultiRpcRequest::new(
            self.providers.services,
            RpcMethod::EthGetTransactionCount,
            params,
            response_size_estimate,
            ResponseTransform::GetTransactionCount,
            reduction_strategy,
        )
    }

    pub fn eth_call(self, params: EthCallParams) -> MultiRpcRequest<EthCallParams, Data> {
        let response_size_estimate = self.response_size_estimate(256 + HEADER_SIZE_LIMIT);
        let reduction_strategy = self.reduction_strategy();
        MultiRpcRequest::new(
            self.providers.services,
            RpcMethod::EthCall,
            params,
            response_size_estimate,
            ResponseTransform::Call,
            reduction_strategy,
        )
    }

    pub fn multi_request(
        self,
        method: RpcMethod,
        params: Option<&Value>,
    ) -> MultiRpcRequest<Option<&Value>, RawJson> {
        let response_size_estimate = self.response_size_estimate(256 + HEADER_SIZE_LIMIT);
        let reduction_strategy = self.reduction_strategy();
        MultiRpcRequest::new(
            self.providers.services,
            method,
            params,
            response_size_estimate,
            ResponseTransform::Raw,
            reduction_strategy,
        )
    }
}

use crate::logs::{DEBUG, INFO};
use crate::rpc_client::eth_rpc::{HttpResponsePayload, ResponseSizeEstimate, HEADER_SIZE_LIMIT};
use crate::rpc_client::numeric::TransactionCount;
use evm_rpc_types::{
    ConsensusStrategy, EthMainnetService, EthSepoliaService, L2MainnetService, ProviderError,
    RpcConfig, RpcError, RpcResult, RpcService, RpcServices,
};
use ic_canister_log::log;
use json::requests::{
    BlockSpec, EthCallParams, FeeHistoryParams, GetBlockByNumberParams, GetLogsParam,
    GetTransactionCountParams,
};
use json::responses::{
    Block, Data, FeeHistory, LogEntry, SendRawTransactionResult, TransactionReceipt,
};
use json::Hash;
use serde::{de::DeserializeOwned, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Debug;

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

    fn consensus_strategy(&self) -> ConsensusStrategy {
        self.config
            .response_consensus
            .as_ref()
            .cloned()
            .unwrap_or_default()
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
        let results = {
            let mut fut = Vec::with_capacity(providers.len());
            for provider in providers {
                log!(DEBUG, "[parallel_call]: will call provider: {:?}", provider);
                fut.push(async {
                    eth_rpc::call::<_, _>(
                        provider,
                        method.clone(),
                        params.clone(),
                        response_size_estimate,
                    )
                    .await
                });
            }
            futures::future::join_all(fut).await
        };
        MultiCallResults::from_non_empty_iter(providers.iter().cloned().zip(results.into_iter()))
    }

    pub async fn eth_get_logs(
        &self,
        params: GetLogsParam,
    ) -> Result<Vec<LogEntry>, MultiCallError<Vec<LogEntry>>> {
        self.parallel_call(
            "eth_getLogs",
            vec![params],
            self.response_size_estimate(1024 + HEADER_SIZE_LIMIT),
        )
        .await
        .reduce(self.consensus_strategy())
    }

    pub async fn eth_get_block_by_number(
        &self,
        block: BlockSpec,
    ) -> Result<Block, MultiCallError<Block>> {
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
    ) -> Result<Option<TransactionReceipt>, MultiCallError<Option<TransactionReceipt>>> {
        self.parallel_call(
            "eth_getTransactionReceipt",
            vec![tx_hash],
            self.response_size_estimate(700 + HEADER_SIZE_LIMIT),
        )
        .await
        .reduce(self.consensus_strategy())
    }

    pub async fn eth_fee_history(
        &self,
        params: FeeHistoryParams,
    ) -> Result<FeeHistory, MultiCallError<FeeHistory>> {
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
    ) -> Result<SendRawTransactionResult, MultiCallError<SendRawTransactionResult>> {
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
    ) -> Result<TransactionCount, MultiCallError<TransactionCount>> {
        self.parallel_call(
            "eth_getTransactionCount",
            params,
            self.response_size_estimate(50 + HEADER_SIZE_LIMIT),
        )
        .await
        .reduce(self.consensus_strategy())
    }

    pub async fn eth_call(&self, params: EthCallParams) -> Result<Data, MultiCallError<Data>> {
        self.parallel_call(
            "eth_call",
            params,
            self.response_size_estimate(256 + HEADER_SIZE_LIMIT),
        )
        .await
        .reduce(self.consensus_strategy())
    }
}

/// Aggregates responses of different providers to the same query.
/// Guaranteed to be non-empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultiCallResults<T> {
    ok_results: BTreeMap<RpcService, T>,
    errors: BTreeMap<RpcService, RpcError>,
}

impl<T> Default for MultiCallResults<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> MultiCallResults<T> {
    pub fn new() -> Self {
        Self {
            ok_results: BTreeMap::new(),
            errors: BTreeMap::new(),
        }
    }

    pub fn from_non_empty_iter<I: IntoIterator<Item = (RpcService, RpcResult<T>)>>(
        iter: I,
    ) -> Self {
        let mut results = Self::new();
        for (provider, result) in iter {
            results.insert_once(provider, result);
        }
        if results.is_empty() {
            panic!("BUG: MultiCallResults cannot be empty!")
        }
        results
    }

    fn is_empty(&self) -> bool {
        self.ok_results.is_empty() && self.errors.is_empty()
    }

    fn insert_once(&mut self, provider: RpcService, result: RpcResult<T>) {
        match result {
            Ok(value) => {
                assert!(!self.errors.contains_key(&provider));
                assert!(self.ok_results.insert(provider, value).is_none());
            }
            Err(error) => {
                assert!(!self.ok_results.contains_key(&provider));
                assert!(self.errors.insert(provider, error).is_none());
            }
        }
    }

    #[cfg(test)]
    fn from_json_rpc_result<
        I: IntoIterator<
            Item = (
                RpcService,
                Result<json::responses::JsonRpcResult<T>, RpcError>,
            ),
        >,
    >(
        iter: I,
    ) -> Self {
        Self::from_non_empty_iter(iter.into_iter().map(|(provider, result)| {
            (
                provider,
                match result {
                    Ok(json_rpc_result) => match json_rpc_result {
                        json::responses::JsonRpcResult::Result(value) => Ok(value),
                        json::responses::JsonRpcResult::Error { code, message } => {
                            Err(RpcError::JsonRpcError(evm_rpc_types::JsonRpcError {
                                code,
                                message,
                            }))
                        }
                    },
                    Err(e) => Err(e),
                },
            )
        }))
    }

    pub fn into_vec(self) -> Vec<(RpcService, RpcResult<T>)> {
        self.ok_results
            .into_iter()
            .map(|(provider, result)| (provider, Ok(result)))
            .chain(
                self.errors
                    .into_iter()
                    .map(|(provider, error)| (provider, Err(error))),
            )
            .collect()
    }

    fn group_errors(&self) -> BTreeMap<&RpcError, BTreeSet<&RpcService>> {
        let mut errors: BTreeMap<_, _> = BTreeMap::new();
        for (provider, error) in self.errors.iter() {
            errors
                .entry(error)
                .or_insert_with(BTreeSet::new)
                .insert(provider);
        }
        errors
    }
}

impl<T: PartialEq> MultiCallResults<T> {
    /// Expects all results to be ok or return the following error:
    /// * MultiCallError::ConsistentError: all errors are the same and there is no ok results.
    /// * MultiCallError::InconsistentResults: in all other cases.
    fn all_ok(self) -> Result<BTreeMap<RpcService, T>, MultiCallError<T>> {
        if self.errors.is_empty() {
            return Ok(self.ok_results);
        }
        Err(self.expect_error())
    }

    fn expect_error(self) -> MultiCallError<T> {
        let errors = self.group_errors();
        match errors.len() {
            0 => {
                panic!("BUG: errors should be non-empty")
            }
            1 if self.ok_results.is_empty() => {
                MultiCallError::ConsistentError(errors.into_keys().next().unwrap().clone())
            }
            _ => MultiCallError::InconsistentResults(self),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum MultiCallError<T> {
    ConsistentError(RpcError),
    InconsistentResults(MultiCallResults<T>),
}

impl<T: Debug + PartialEq + Clone + Serialize> MultiCallResults<T> {
    pub fn reduce(self, strategy: ConsensusStrategy) -> Result<T, MultiCallError<T>> {
        match strategy {
            ConsensusStrategy::Equality => self.reduce_with_equality(),
            ConsensusStrategy::Threshold { total: _, min } => self.reduce_with_threshold(min),
        }
    }

    fn reduce_with_equality(self) -> Result<T, MultiCallError<T>> {
        let mut results = self.all_ok()?.into_iter();
        let (base_node_provider, base_result) = results
            .next()
            .expect("BUG: MultiCallResults is guaranteed to be non-empty");
        let mut inconsistent_results: Vec<_> = results
            .filter(|(_provider, result)| result != &base_result)
            .collect();
        if !inconsistent_results.is_empty() {
            inconsistent_results.push((base_node_provider, base_result));
            let error = MultiCallError::InconsistentResults(MultiCallResults::from_non_empty_iter(
                inconsistent_results
                    .into_iter()
                    .map(|(provider, result)| (provider, Ok(result))),
            ));
            log!(
                INFO,
                "[reduce_with_equality]: inconsistent results {error:?}"
            );
            return Err(error);
        }
        Ok(base_result)
    }

    fn reduce_with_threshold(self, min: u8) -> Result<T, MultiCallError<T>> {
        assert!(min > 0, "BUG: min must be greater than 0");
        if self.ok_results.len() < min as usize {
            // At least total >= min were queried,
            // so there is at least one error
            return Err(self.expect_error());
        }
        let distribution = ResponseDistribution::from_non_empty_iter(self.ok_results.clone());
        let (most_likely_response, providers) = distribution
            .most_frequent()
            .expect("BUG: distribution should be non-empty");
        if providers.len() >= min as usize {
            Ok(most_likely_response.clone())
        } else {
            log!(
                INFO,
                "[reduce_with_threshold]: too many inconsistent ok responses to reach threshold of {min}, results: {self:?}"
            );
            Err(MultiCallError::InconsistentResults(self))
        }
    }
}

/// Distribution of responses observed from different providers.
///
/// From the API point of view, it emulates a map from a response instance to a set of providers that returned it.
/// At the implementation level, to avoid requiring `T` to have a total order (i.e., must implements `Ord` if it were to be used as keys in a `BTreeMap`) which might not always be meaningful,
/// we use as key the hash of the serialized response instance.
struct ResponseDistribution<T> {
    hashes: BTreeMap<[u8; 32], T>,
    responses: BTreeMap<[u8; 32], BTreeSet<RpcService>>,
}

impl<T> Default for ResponseDistribution<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> ResponseDistribution<T> {
    pub fn new() -> Self {
        Self {
            hashes: BTreeMap::new(),
            responses: BTreeMap::new(),
        }
    }

    /// Returns the most frequent response and the set of providers that returned it.
    pub fn most_frequent(&self) -> Option<(&T, &BTreeSet<RpcService>)> {
        self.responses
            .iter()
            .max_by_key(|(_hash, providers)| providers.len())
            .map(|(hash, providers)| {
                (
                    self.hashes.get(hash).expect("BUG: hash should be present"),
                    providers,
                )
            })
    }
}

impl<T: Debug + PartialEq + Serialize> ResponseDistribution<T> {
    pub fn from_non_empty_iter<I: IntoIterator<Item = (RpcService, T)>>(iter: I) -> Self {
        let mut distribution = Self::new();
        for (provider, result) in iter {
            distribution.insert_once(provider, result);
        }
        distribution
    }

    pub fn insert_once(&mut self, provider: RpcService, result: T) {
        use ic_sha3::Keccak256;
        let hash = Keccak256::hash(serde_json::to_vec(&result).expect("BUG: failed to serialize"));
        match self.hashes.get(&hash) {
            Some(existing_result) => {
                assert_eq!(
                    existing_result, &result,
                    "BUG: different results once serialized have the same hash"
                );
                let providers = self
                    .responses
                    .get_mut(&hash)
                    .expect("BUG: hash is guaranteed to be present");
                assert!(
                    providers.insert(provider),
                    "BUG: provider is already present"
                );
            }
            None => {
                assert_eq!(self.hashes.insert(hash, result), None);
                let providers = BTreeSet::from_iter(std::iter::once(provider));
                assert_eq!(self.responses.insert(hash, providers), None);
            }
        }
    }
}

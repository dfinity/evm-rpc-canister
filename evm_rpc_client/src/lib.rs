//! Client to interact with the EVM RPC canister
//!
//! # Examples
//!
//! ## Configuring the client
//!
//! By default, any RPC endpoint supported by the EVM RPC canister will call 3 providers and require
//! equality between their results. It is possible to customize the client so that another strategy,
//! such as 2-out-of-3 in the example below, is used for all following calls.
//!
//! ```rust
//! use evm_rpc_client::EvmRpcClient;
//! use evm_rpc_types::{ConsensusStrategy, RpcConfig, RpcServices};
//!
//! let client = EvmRpcClient::builder_for_ic()
//!     .with_rpc_sources(RpcServices::EthMainnet(None))
//!     .with_consensus_strategy(ConsensusStrategy::Threshold {
//!         total: Some(3),
//!         min: 2,
//!     })
//!     .build();
//! ```
//!
//! By default, the client will return Candid output types for all calls. It is also possible to
//! customize the client so that it returns [alloy](alloy.rs) types instead. Note that this requires
//! the `alloy` Cargo feature to be enabled.
//!
//! ```rust
//! use evm_rpc_client::EvmRpcClient;
//!
//! let client = EvmRpcClient::builder_for_ic()
//!     .with_alloy()
//!     .build();
//! ```
//!
//! ## Specifying the amount of cycles to send
//!
//! Every call made to the EVM RPC canister that triggers HTTPs outcalls (e.g., `eth_getLogs`)
//! needs to attach some cycles to pay for the call.
//! By default, the client will attach some amount of cycles that should be sufficient for most cases.
//!
//! If this is not the case, the amount of cycles to be sent can be overridden. It's advisable to
//! actually send *more* cycles than required, since *unused cycles will be refunded*.
//!
//! ```rust
//! use alloy_primitives::{address, U256};
//! use alloy_rpc_types::BlockNumberOrTag;
//! use evm_rpc_client::EvmRpcClient;
//! use evm_rpc_types::MultiRpcResult;
//!
//! # use evm_rpc_types::Nat256;
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let client = EvmRpcClient::builder_for_ic()
//!     .with_alloy()
//! #   .with_default_stub_response(MultiRpcResult::Consistent(Ok(Nat256::from(1_u64))))
//!     .build();
//!
//! let result = client
//!     .get_transaction_count((
//!         address!("0xdac17f958d2ee523a2206206994597c13d831ec7"),
//!         BlockNumberOrTag::Latest,
//!     ))
//!     .with_cycles(20_000_000_000)
//!     .send()
//!     .await
//!     .expect_consistent();
//!
//! assert_eq!(result, Ok(U256::ONE));
//! # Ok(())
//! # }
//! ```
//!
//! ## Overriding client configuration for a specific call
//!
//! Besides changing the amount of cycles for a particular call as described above,
//! it is sometimes desirable to have a custom configuration for a specific
//! call that is different from the one used by the client for all the other calls.
//!
//! For example, maybe for most calls, a 2 out-of 3 strategy is good enough, but for `eth_getLogs`
//! your application requires a higher threshold and more robustness with a 3-out-of-5 :
//!
//! ```rust
//! use alloy_primitives::{address, U256};
//! use alloy_rpc_types::BlockNumberOrTag;
//! use evm_rpc_client::EvmRpcClient;
//! use evm_rpc_types::{ConsensusStrategy, MultiRpcResult, RpcServices};
//!
//! # use evm_rpc_types::Nat256;
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let client = EvmRpcClient::builder_for_ic()
//! #   .with_default_stub_response(MultiRpcResult::Consistent(Ok(Nat256::from(1_u64))))
//!     .with_alloy()
//!     .with_rpc_sources(RpcServices::EthMainnet(None))
//!     .with_consensus_strategy(ConsensusStrategy::Threshold {
//!         total: Some(3),
//!         min: 2,
//!     })
//!     .build();
//!
//! let result = client
//!     .get_transaction_count((
//!         address!("0xdac17f958d2ee523a2206206994597c13d831ec7"),
//!         BlockNumberOrTag::Latest,
//!     ))
//!     .with_cycles(20_000_000_000)
//!     .send()
//!     .await
//!     .expect_consistent();
//!
//! assert_eq!(result, Ok(U256::ONE));
//! # Ok(())
//! # }
//! ```

#![forbid(unsafe_code)]
#![forbid(missing_docs)]

#[cfg(not(target_arch = "wasm32"))]
pub mod fixtures;
mod request;
mod runtime;

use candid::{CandidType, Principal};
use evm_rpc_types::{
    BlockTag, CallArgs, ConsensusStrategy, FeeHistoryArgs, GetLogsArgs, GetTransactionCountArgs,
    Hex, Hex32, RpcConfig, RpcServices,
};
use ic_error_types::RejectCode;
#[cfg(feature = "alloy")]
pub use request::alloy::AlloyResponseConverter;
use request::{
    CallRequest, CallRequestBuilder, EvmRpcResponseConverter, FeeHistoryRequest,
    FeeHistoryRequestBuilder, GetBlockByNumberRequest, GetBlockByNumberRequestBuilder,
    GetLogsRequest, GetLogsRequestBuilder, GetTransactionCountRequest,
    GetTransactionCountRequestBuilder, GetTransactionReceiptRequest,
    GetTransactionReceiptRequestBuilder, JsonRequest, JsonRequestBuilder, Request, RequestBuilder,
    SendRawTransactionRequest, SendRawTransactionRequestBuilder,
};
pub use request::{CandidResponseConverter, EvmRpcConfig};
pub use runtime::{IcRuntime, Runtime};
use serde::de::DeserializeOwned;
use std::sync::Arc;

/// The principal identifying the productive EVM RPC canister under NNS control.
///
/// ```rust
/// use candid::Principal;
/// use evm_rpc_client::EVM_RPC_CANISTER;
///
/// assert_eq!(EVM_RPC_CANISTER, Principal::from_text("7hfb6-caaaa-aaaar-qadga-cai").unwrap())
/// ```
pub const EVM_RPC_CANISTER: Principal = Principal::from_slice(&[0, 0, 0, 0, 2, 48, 0, 204, 1, 1]);

/// Client to interact with the EVM RPC canister.
#[derive(Debug)]
pub struct EvmRpcClient<R, C> {
    config: Arc<ClientConfig<R, C>>,
}

impl<R> EvmRpcClient<R, CandidResponseConverter> {
    /// Creates a [`ClientBuilder`] to configure a [`EvmRpcClient`].
    pub fn builder(
        runtime: R,
        evm_rpc_canister: Principal,
    ) -> ClientBuilder<R, CandidResponseConverter> {
        ClientBuilder::new(runtime, evm_rpc_canister)
    }
}

impl<R, C> Clone for EvmRpcClient<R, C> {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
        }
    }
}

impl EvmRpcClient<IcRuntime, CandidResponseConverter> {
    /// Creates a [`ClientBuilder`] to configure a [`EvmRpcClient`] targeting [`EVM_RPC_CANISTER`]
    /// running on the Internet Computer.
    pub fn builder_for_ic() -> ClientBuilder<IcRuntime, CandidResponseConverter> {
        ClientBuilder::new(IcRuntime, EVM_RPC_CANISTER)
    }
}

/// Configuration for the EVM RPC canister client.
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct ClientConfig<R, C> {
    runtime: R,
    evm_rpc_canister: Principal,
    rpc_config: Option<RpcConfig>,
    rpc_services: RpcServices,
    response_converter: C,
}

/// A [`ClientBuilder`] to create a [`EvmRpcClient`] with custom configuration.
#[must_use]
pub struct ClientBuilder<R, C> {
    config: ClientConfig<R, C>,
}

impl<R: Clone, C: Clone> Clone for ClientBuilder<R, C> {
    fn clone(&self) -> Self {
        ClientBuilder {
            config: self.config.clone(),
        }
    }
}

impl<R> ClientBuilder<R, CandidResponseConverter> {
    fn new(runtime: R, evm_rpc_canister: Principal) -> ClientBuilder<R, CandidResponseConverter> {
        Self {
            config: ClientConfig {
                runtime,
                evm_rpc_canister,
                rpc_config: None,
                rpc_services: RpcServices::EthMainnet(None),
                response_converter: CandidResponseConverter,
            },
        }
    }
}

impl<R, C> ClientBuilder<R, C> {
    /// Modify the existing runtime by applying a transformation function.
    ///
    /// The transformation does not necessarily produce a runtime of the same type.
    pub fn with_runtime<S, F: FnOnce(R) -> S>(self, other_runtime: F) -> ClientBuilder<S, C> {
        ClientBuilder {
            config: ClientConfig {
                runtime: other_runtime(self.config.runtime),
                evm_rpc_canister: self.config.evm_rpc_canister,
                rpc_config: self.config.rpc_config,
                rpc_services: self.config.rpc_services,
                response_converter: self.config.response_converter,
            },
        }
    }

    /// Mutates the builder to use the given [`RpcServices`].
    pub fn with_rpc_sources(mut self, rpc_services: RpcServices) -> Self {
        self.config.rpc_services = rpc_services;
        self
    }

    /// Mutates the builder to use the given [`RpcConfig`].
    pub fn with_rpc_config(mut self, rpc_config: RpcConfig) -> Self {
        self.config.rpc_config = Some(rpc_config);
        self
    }

    /// Mutates the builder to use the given [`ConsensusStrategy`] in the [`RpcConfig`].
    pub fn with_consensus_strategy(mut self, consensus_strategy: ConsensusStrategy) -> Self {
        self.config.rpc_config = Some(RpcConfig {
            response_consensus: Some(consensus_strategy),
            ..self.config.rpc_config.unwrap_or_default()
        });
        self
    }

    /// Mutates the builder to use the given `response_size_estimate` in the [`RpcConfig`].
    pub fn with_response_size_estimate(mut self, response_size_estimate: u64) -> Self {
        self.config.rpc_config = Some(RpcConfig {
            response_size_estimate: Some(response_size_estimate),
            ..self.config.rpc_config.unwrap_or_default()
        });
        self
    }

    /// Mutates the builder to create a client with [alloy](https://alloy.rs/) response types.
    #[cfg(feature = "alloy")]
    pub fn with_alloy(self) -> ClientBuilder<R, AlloyResponseConverter> {
        ClientBuilder {
            config: ClientConfig {
                runtime: self.config.runtime,
                evm_rpc_canister: self.config.evm_rpc_canister,
                rpc_config: self.config.rpc_config,
                rpc_services: self.config.rpc_services,
                response_converter: AlloyResponseConverter,
            },
        }
    }

    /// Mutates the builder to create a client with Candid response types.
    pub fn with_candid(self) -> ClientBuilder<R, CandidResponseConverter> {
        ClientBuilder {
            config: ClientConfig {
                runtime: self.config.runtime,
                evm_rpc_canister: self.config.evm_rpc_canister,
                rpc_config: self.config.rpc_config,
                rpc_services: self.config.rpc_services,
                response_converter: CandidResponseConverter,
            },
        }
    }

    /// Creates a [`EvmRpcClient`] from the configuration specified in the [`ClientBuilder`].
    pub fn build(self) -> EvmRpcClient<R, C> {
        EvmRpcClient {
            config: Arc::new(self.config),
        }
    }
}

impl<R, C: EvmRpcResponseConverter> EvmRpcClient<R, C> {
    /// Call `eth_call` on the EVM RPC canister.
    ///
    /// # Examples
    ///
    /// This example sends an `eth_call` to the USDC ERC-20 contract to fetch its symbol,
    /// then decodes the ABI-encoded response into the human-readable string `USDC`.
    ///
    /// ```rust
    /// use alloy_dyn_abi::{DynSolType, DynSolValue};
    /// use alloy_primitives::{address, bytes, Bytes};
    /// use alloy_rpc_types::BlockNumberOrTag;
    /// use evm_rpc_client::EvmRpcClient;
    ///
    /// # use evm_rpc_types::{Hex, MultiRpcResult};
    /// # use std::str::FromStr;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = EvmRpcClient::builder_for_ic()
    ///     .with_alloy()
    /// #   .with_default_stub_response(MultiRpcResult::Consistent(Ok(
    /// #       Hex::from_str("0x000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000045553444300000000000000000000000000000000000000000000000000000000").unwrap()
    /// #   )))
    ///     .build();
    ///
    /// let tx_request = alloy_rpc_types::TransactionRequest::default()
    ///     // USDC address
    ///     .from(address!("0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48"))
    ///     // Selector for `symbol()`
    ///     .input(bytes!(0x95, 0xd8, 0x9b, 0x41).into());
    ///
    /// let result = client
    ///     .call(tx_request)
    ///     .with_block(BlockNumberOrTag::Latest)
    ///     .send()
    ///     .await
    ///     .expect_consistent()
    ///     .unwrap();
    ///
    /// let decoded = DynSolType::String.abi_decode(&result);
    /// assert_eq!(decoded, Ok(DynSolValue::from("USDC".to_string())));
    /// # Ok(())
    /// # }
    /// ```
    pub fn call<T>(&self, params: T) -> CallRequestBuilder<R, C, C::CallOutput>
    where
        T: TryInto<CallArgs>,
        <T as TryInto<CallArgs>>::Error: std::fmt::Debug,
    {
        RequestBuilder::new(
            self.clone(),
            CallRequest::new(
                params
                    .try_into()
                    .unwrap_or_else(|e| panic!("Invalid transaction request: {e:?}")),
            ),
            10_000_000_000,
        )
    }

    /// Call `eth_getBlockByNumber` on the EVM RPC canister.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use alloy_primitives::{address, b256, bytes};
    /// use alloy_rpc_types::{Block, BlockNumberOrTag};
    /// use evm_rpc_client::EvmRpcClient;
    ///
    /// # use evm_rpc_types::{Hex, Hex20, Hex32, Hex256, MultiRpcResult, Nat256};
    /// # use std::str::FromStr;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = EvmRpcClient::builder_for_ic()
    ///     .with_alloy()
    /// #   .with_default_stub_response(MultiRpcResult::Consistent(Ok(evm_rpc_types::Block {
    /// #       base_fee_per_gas: None,
    /// #       number: Nat256::ZERO,
    /// #       difficulty: Some(Nat256::ZERO),
    /// #       extra_data: Hex::from(vec![]),
    /// #       gas_limit: Nat256::ZERO,
    /// #       gas_used: Nat256::ZERO,
    /// #       hash: Hex32::from(b256!("0x47302c2ebfb29611c74f917a380f3cf45c9dfe9de3554e18bff9a9ca7c8454e2")),
    /// #       logs_bloom: Hex256::from([0; 256]),
    /// #       miner: Hex20::from([0; 20]),
    /// #       mix_hash: Hex32::from([0; 32]),
    /// #       nonce: Nat256::ZERO,
    /// #       parent_hash: Hex32::from([0; 32]),
    /// #       receipts_root: Hex32::from([0; 32]),
    /// #       sha3_uncles: Hex32::from([0; 32]),
    /// #       size: Nat256::ZERO,
    /// #       state_root: Hex32::from([0; 32]),
    /// #       timestamp: Nat256::ZERO,
    /// #       total_difficulty: Some(Nat256::ZERO),
    /// #       transactions: vec![],
    /// #       transactions_root: Some(Hex32::from([0; 32])),
    /// #       uncles: vec![],
    /// #   })))
    ///     .build();
    ///
    /// let result = client
    ///     .get_block_by_number(BlockNumberOrTag::Number(23225439))
    ///     .send()
    ///     .await
    ///     .expect_consistent()
    ///     .unwrap();
    ///
    /// assert_eq!(result.hash(), b256!("0x47302c2ebfb29611c74f917a380f3cf45c9dfe9de3554e18bff9a9ca7c8454e2"));
    /// # Ok(())
    /// # }
    /// ```
    pub fn get_block_by_number(
        &self,
        params: impl Into<BlockTag>,
    ) -> GetBlockByNumberRequestBuilder<R, C, C::GetBlockByNumberOutput> {
        RequestBuilder::new(
            self.clone(),
            GetBlockByNumberRequest::new(params.into()),
            10_000_000_000,
        )
    }

    /// Call `eth_feeHistory` on the EVM RPC canister.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use alloy_rpc_types::{BlockNumberOrTag, FeeHistory};
    /// use evm_rpc_client::EvmRpcClient;
    ///
    /// # use evm_rpc_types::MultiRpcResult;
    /// # use std::str::FromStr;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = EvmRpcClient::builder_for_ic()
    ///     .with_alloy()
    /// #   .with_default_stub_response(MultiRpcResult::Consistent(Ok(evm_rpc_types::FeeHistory {
    /// #       oldest_block: 0x1627fb8_u64.into(),
    /// #       base_fee_per_gas: vec![
    /// #           0x2e9d4aab_u128.into(),
    /// #           0x2fcec030_u128.into(),
    /// #           0x2ea50b1a_u128.into(),
    /// #           0x2e0a7fbd_u128.into(),
    /// #       ],
    /// #       gas_used_ratio: vec![
    /// #           0.6023888561516908_f64,
    /// #           0.4027000776793981,
    /// #           0.44823085535879276,
    /// #       ],
    /// #       reward: vec![
    /// #           vec![0xe4e1c0_u128.into(), 0x05f5e100_u128.into(), 0x59682f00_u128.into()],
    /// #           vec![0x011170_u128.into(), 0x05d628d0_u128.into(), 0x77359400_u128.into()],
    /// #           vec![0x0222e0_u128.into(), 0x3b9aca00_u128.into(), 0x77359400_u128.into()],
    /// #       ],
    /// #   })))
    ///     .build();
    ///
    /// let result = client
    ///     .fee_history((0x3_u64, BlockNumberOrTag::Latest))
    ///     .send()
    ///     .await
    ///     .expect_consistent()
    ///     .unwrap();
    ///
    /// assert_eq!(result, FeeHistory {
    ///     oldest_block: 0x1627fb8_u64.into(),
    ///     base_fee_per_gas: vec![
    ///         0x2e9d4aab_u128,
    ///         0x2fcec030,
    ///         0x2ea50b1a,
    ///         0x2e0a7fbd,
    ///     ],
    ///     gas_used_ratio: vec![
    ///         0.6023888561516908_f64,
    ///         0.4027000776793981,
    ///         0.44823085535879276,
    ///     ],
    ///     reward: Some(vec![
    ///         vec![0xe4e1c0_u128, 0x05f5e100, 0x59682f00],
    ///         vec![0x011170_u128, 0x05d628d0, 0x77359400],
    ///         vec![0x0222e0_u128, 0x3b9aca00, 0x77359400],
    ///     ]),
    ///     base_fee_per_blob_gas: vec![],
    ///     blob_gas_used_ratio: vec![],
    /// });
    /// # Ok(())
    /// # }
    /// ```
    pub fn fee_history(
        &self,
        params: impl Into<FeeHistoryArgs>,
    ) -> FeeHistoryRequestBuilder<R, C, C::FeeHistoryOutput> {
        RequestBuilder::new(
            self.clone(),
            FeeHistoryRequest::new(params.into()),
            10_000_000_000,
        )
    }

    /// Call `eth_getLogs` on the EVM RPC canister.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use alloy_primitives::{address, b256, bytes};
    /// use alloy_rpc_types::Log;
    /// use evm_rpc_client::EvmRpcClient;
    ///
    /// # use evm_rpc_types::{Hex, Hex20, Hex32, MultiRpcResult};
    /// # use std::str::FromStr;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = EvmRpcClient::builder_for_ic()
    ///     .with_alloy()
    /// #   .with_default_stub_response(MultiRpcResult::Consistent(Ok(vec![
    /// #       evm_rpc_types::LogEntry {
    /// #           address: Hex20::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap(),
    /// #           topics: vec![
    /// #               Hex32::from_str("0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef").unwrap(),
    /// #               Hex32::from_str("0x000000000000000000000000000000000004444c5dc75cb358380d2e3de08a90").unwrap(),
    /// #               Hex32::from_str("0x0000000000000000000000000000000aa232009084bd71a5797d089aa4edfad4").unwrap(),
    /// #           ],
    /// #           data: Hex::from_str("0x00000000000000000000000000000000000000000000000000000000cd566ae8").unwrap(),
    /// #           block_number: Some(0x161bd70_u64.into()),
    /// #           transaction_hash: Some(Hex32::from_str("0xfe5bc88d0818b66a67b0619b1b4d81bfe38029e3799c7f0eb86b33ca7dc4c811").unwrap()),
    /// #           transaction_index: Some(0x0_u64.into()),
    /// #           block_hash: Some(Hex32::from_str("0x0bbd9b12140e674cdd55e63539a25df8280a70cee3676c94d8e05fa5f868a914").unwrap()),
    /// #           log_index: Some(0x0_u64.into()),
    /// #           removed: false,
    /// #       }
    /// #   ])))
    ///     .build();
    ///
    /// let result = client
    ///     .get_logs(vec![address!("0xdac17f958d2ee523a2206206994597c13d831ec7")])
    ///     .send()
    ///     .await
    ///     .expect_consistent();
    ///
    /// assert_eq!(result.unwrap().first(), Some(
    ///     &Log {
    ///         inner: alloy_primitives::Log {
    ///             address: address!("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"),
    ///             data: alloy_primitives::LogData::new(
    ///                 vec![
    ///                     b256!("0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef"),
    ///                     b256!("0x000000000000000000000000000000000004444c5dc75cb358380d2e3de08a90"),
    ///                     b256!("0x0000000000000000000000000000000aa232009084bd71a5797d089aa4edfad4"),
    ///                 ],
    ///                 bytes!("0x00000000000000000000000000000000000000000000000000000000cd566ae8"),
    ///             ).unwrap(),
    ///         },
    ///         block_hash: Some(b256!("0x0bbd9b12140e674cdd55e63539a25df8280a70cee3676c94d8e05fa5f868a914")),
    ///         block_number: Some(0x161bd70_u64),
    ///         block_timestamp: None,
    ///         transaction_hash: Some(b256!("0xfe5bc88d0818b66a67b0619b1b4d81bfe38029e3799c7f0eb86b33ca7dc4c811")),
    ///         transaction_index: Some(0x0_u64),
    ///         log_index: Some(0x0_u64),
    ///         removed: false,
    ///     },
    /// ));
    /// # Ok(())
    /// # }
    /// ```
    pub fn get_logs(
        &self,
        params: impl Into<GetLogsArgs>,
    ) -> GetLogsRequestBuilder<R, C, C::GetLogsOutput> {
        RequestBuilder::new(
            self.clone(),
            GetLogsRequest::new(params.into()),
            10_000_000_000,
        )
    }

    /// Call `eth_getTransactionCount` on the EVM RPC canister.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use alloy_primitives::{address, U256};
    /// use alloy_rpc_types::BlockNumberOrTag;
    /// use evm_rpc_client::EvmRpcClient;
    ///
    /// # use evm_rpc_types::{MultiRpcResult, Nat256};
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = EvmRpcClient::builder_for_ic()
    ///     .with_alloy()
    /// #   .with_default_stub_response(MultiRpcResult::Consistent(Ok(Nat256::from(1_u64))))
    ///     .build();
    ///
    /// let result = client
    ///     .get_transaction_count((
    ///         address!("0xdac17f958d2ee523a2206206994597c13d831ec7"),
    ///         BlockNumberOrTag::Latest,
    ///     ))
    ///     .send()
    ///     .await
    ///     .expect_consistent();
    ///
    /// assert_eq!(result, Ok(U256::ONE));
    /// # Ok(())
    /// # }
    /// ```
    pub fn get_transaction_count(
        &self,
        params: impl Into<GetTransactionCountArgs>,
    ) -> GetTransactionCountRequestBuilder<R, C, C::GetTransactionCountOutput> {
        RequestBuilder::new(
            self.clone(),
            GetTransactionCountRequest::new(params.into()),
            10_000_000_000,
        )
    }

    /// Call `eth_getTransactionReceipt` on the EVM RPC canister.
    ///
    /// # Examples
    ///
    ///  ```rust
    /// use alloy_primitives::b256;
    /// use alloy_rpc_types::TransactionReceipt;
    /// use evm_rpc_client::EvmRpcClient;
    ///
    /// # use evm_rpc_types::{Hex20, Hex32, Hex256, HexByte, MultiRpcResult, Nat256};
    /// # use std::str::FromStr;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = EvmRpcClient::builder_for_ic()
    ///     .with_alloy()
    /// #   .with_default_stub_response(MultiRpcResult::Consistent(Ok(evm_rpc_types::TransactionReceipt {
    /// #       block_hash: Hex32::from_str("0xf6084155ff2022773b22df3217d16e9df53cbc42689b27ca4789e06b6339beb2").unwrap(),
    /// #       block_number: Nat256::from(0x52a975_u64),
    /// #       effective_gas_price: Nat256::from(0x6052340_u64),
    /// #       gas_used: Nat256::from(0x1308c_u64),
    /// #       cumulative_gas_used: Nat256::from(0x797db0_u64),
    /// #       status: Some(Nat256::from(0x1_u8)),
    /// #       root: None,
    /// #       transaction_hash: Hex32::from_str("0xa3ece39ae137617669c6933b7578b94e705e765683f260fcfe30eaa41932610f").unwrap(),
    /// #       contract_address: None,
    /// #       from: Hex20::from_str("0xd907941c8b3b966546fc408b8c942eb10a4f98df").unwrap(),
    /// #       // This receipt contains some transactions, but they are left out here since not asserted in the doctest
    /// #       logs: vec![],
    /// #       logs_bloom: Hex256::from_str("0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000008000000000000000000000000000000000000000000000000000000000000008000000000000000000000000000000000000000000000000020000000000000000000800000000000000004010000010100000000000000000000000000000000000000000000000000040000080000000000000080000000000000000000000000000000000000000000020000000000000000000000002000000000000000000000000000000000000000000000000000020000000010000000000000000000000000000000000000000000000000000000000").unwrap(),
    /// #       to: Some(Hex20::from_str("0xd6df5935cd03a768b7b9e92637a01b25e24cb709").unwrap()),
    /// #       transaction_index: Nat256::from(0x29_u64),
    /// #       tx_type: HexByte::from(0x0_u8),
    /// #   })))
    ///     .build();
    ///
    /// let result = client
    ///     .get_transaction_receipt(b256!("0xa3ece39ae137617669c6933b7578b94e705e765683f260fcfe30eaa41932610f"))
    ///     .send()
    ///     .await
    ///     .expect_consistent()
    ///     .unwrap();
    ///
    /// assert!(result.unwrap().status());
    /// # Ok(())
    /// # }
    /// ```
    pub fn get_transaction_receipt(
        &self,
        params: impl Into<Hex32>,
    ) -> GetTransactionReceiptRequestBuilder<R, C, C::GetTransactionReceiptOutput> {
        RequestBuilder::new(
            self.clone(),
            GetTransactionReceiptRequest::new(params.into()),
            10_000_000_000,
        )
    }

    /// Call `multi_request` on the EVM RPC canister.
    ///
    /// Note: The EVM RPC canister overrides the `id` field in the JSON-RPC
    /// request payload with the next value from its internal sequential counter.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use alloy_primitives::U256;
    /// use evm_rpc_client::EvmRpcClient;
    /// use serde_json::json;
    /// use std::str::FromStr;
    ///
    /// # use evm_rpc_types::MultiRpcResult;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = EvmRpcClient::builder_for_ic()
    /// #   .with_default_stub_response(MultiRpcResult::Consistent(Ok("0x24604d0d".to_string())))
    ///     .build();
    ///
    /// let result = client
    ///     .multi_request(json!({
    ///         "jsonrpc": "2.0",
    ///         // This value is overwritten by the EVM RPC canister
    ///         "id": 73,
    ///         "method": "eth_gasPrice",
    ///     }))
    ///     .send()
    ///     .await
    ///     .expect_consistent()
    ///     .map(|result| U256::from_str(&result).unwrap())
    ///     .unwrap();
    ///
    /// assert_eq!(result, U256::from(0x24604d0d_u64));
    /// # Ok(())
    /// # }
    /// ```
    pub fn multi_request(
        &self,
        params: serde_json::Value,
    ) -> JsonRequestBuilder<R, C, C::JsonRequestOutput> {
        RequestBuilder::new(
            self.clone(),
            JsonRequest::try_from(params).expect("Client error: invalid JSON request"),
            10_000_000_000,
        )
    }

    /// Call `eth_sendRawTransaction` on the EVM RPC canister.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use alloy_primitives::{b256, B256, bytes};
    /// use evm_rpc_client::EvmRpcClient;
    ///
    /// # use evm_rpc_types::{Hex32, MultiRpcResult, SendRawTransactionStatus};
    /// # use std::str::FromStr;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = EvmRpcClient::builder_for_ic()
    ///     .with_alloy()
    /// #   .with_default_stub_response(MultiRpcResult::Consistent(Ok(SendRawTransactionStatus::Ok(Some(Hex32::from_str("0x33469b22e9f636356c4160a87eb19df52b7412e8eac32a4a55ffe88ea8350788").unwrap())))))
    ///     .build();
    ///
    /// let result = client
    ///     .send_raw_transaction(bytes!("0xf86c098504a817c800825208943535353535353535353535353535353535353535880de0b6b3a76400008025a028ef61340bd939bc2195fe537567866003e1a15d3c71ff63e1590620aa636276a067cbe9d8997f761aecb703304b3800ccf555c9f3dc64214b297fb1966a3b6d83"))
    ///     .send()
    ///     .await
    ///     .expect_consistent();
    ///
    /// assert_eq!(result, Ok(b256!("0x33469b22e9f636356c4160a87eb19df52b7412e8eac32a4a55ffe88ea8350788")));
    /// # Ok(())
    /// # }
    /// ```
    pub fn send_raw_transaction(
        &self,
        params: impl Into<Hex>,
    ) -> SendRawTransactionRequestBuilder<R, C, C::SendRawTransactionOutput> {
        RequestBuilder::new(
            self.clone(),
            SendRawTransactionRequest::new(params.into()),
            10_000_000_000,
        )
    }
}

impl<R: Runtime, C> EvmRpcClient<R, C> {
    async fn execute_request<Config, Params, CandidOutput, Output>(
        &self,
        request: Request<Config, Params, CandidOutput, Output>,
    ) -> Output
    where
        Config: CandidType + Send,
        Params: CandidType + Send,
        CandidOutput: Into<Output> + CandidType + DeserializeOwned,
    {
        let rpc_method = request.endpoint.rpc_method();
        self.try_execute_request(request)
            .await
            .unwrap_or_else(|e| panic!("Client error: failed to call `{}`: {e:?}", rpc_method))
    }

    async fn try_execute_request<Config, Params, CandidOutput, Output>(
        &self,
        request: Request<Config, Params, CandidOutput, Output>,
    ) -> Result<Output, (RejectCode, String)>
    where
        Config: CandidType + Send,
        Params: CandidType + Send,
        CandidOutput: Into<Output> + CandidType + DeserializeOwned,
    {
        self.config
            .runtime
            .update_call::<(RpcServices, Option<Config>, Params), CandidOutput>(
                self.config.evm_rpc_canister,
                request.endpoint.rpc_method(),
                (request.rpc_services, request.rpc_config, request.params),
                request.cycles,
            )
            .await
            .map(Into::into)
    }
}

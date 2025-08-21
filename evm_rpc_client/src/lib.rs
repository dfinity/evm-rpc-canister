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
//! # // TODO XC-412: Use simpler example e.g. `eth_getBalance`
//! use alloy_primitives::{address, b256, bytes};
//! use evm_rpc_client::EvmRpcClient;
//!
//! # use evm_rpc_types::{Hex, Hex20, Hex32, MultiRpcResult};
//! # use std::str::FromStr;
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let client = EvmRpcClient::builder_for_ic()
//! #   .with_default_stub_response(MultiRpcResult::Consistent(Ok(vec![
//! #       evm_rpc_types::LogEntry {
//! #           address: Hex20::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap(),
//! #           topics: vec![
//! #               Hex32::from_str("0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef").unwrap(),
//! #               Hex32::from_str("0x000000000000000000000000000000000004444c5dc75cb358380d2e3de08a90").unwrap(),
//! #               Hex32::from_str("0x0000000000000000000000000000000aa232009084bd71a5797d089aa4edfad4").unwrap(),
//! #           ],
//! #           data: Hex::from_str("0x00000000000000000000000000000000000000000000000000000000cd566ae8").unwrap(),
//! #           block_number: Some(0x161bd70_u64.into()),
//! #           transaction_hash: Some(Hex32::from_str("0xfe5bc88d0818b66a67b0619b1b4d81bfe38029e3799c7f0eb86b33ca7dc4c811").unwrap()),
//! #           transaction_index: Some(0x0_u64.into()),
//! #           block_hash: Some(Hex32::from_str("0x0bbd9b12140e674cdd55e63539a25df8280a70cee3676c94d8e05fa5f868a914").unwrap()),
//! #           log_index: Some(0x0_u64.into()),
//! #           removed: false,
//! #       }
//! #   ])))
//!     .build();
//!
//! let request = client.get_logs(vec![address!("0xdac17f958d2ee523a2206206994597c13d831ec7")]);
//!
//! let result = request
//!     .with_cycles(10_000_000_000)
//!     .send()
//!     .await
//!     .expect_consistent();
//!
//! assert!(result.is_ok());
//! assert_eq!(result.unwrap().first(), Some(
//!     &alloy_rpc_types::Log {
//!         inner: alloy_primitives::Log {
//!             address: address!("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"),
//!             data: alloy_primitives::LogData::new(
//!                 vec![
//!                     b256!("0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef"),
//!                     b256!("0x000000000000000000000000000000000004444c5dc75cb358380d2e3de08a90"),
//!                     b256!("0x0000000000000000000000000000000aa232009084bd71a5797d089aa4edfad4"),
//!                 ],
//!                 bytes!("0x00000000000000000000000000000000000000000000000000000000cd566ae8"),
//!             ).unwrap(),
//!         },
//!         block_hash: Some(b256!("0x0bbd9b12140e674cdd55e63539a25df8280a70cee3676c94d8e05fa5f868a914")),
//!         block_number: Some(0x161bd70_u64),
//!         block_timestamp: None,
//!         transaction_hash: Some(b256!("0xfe5bc88d0818b66a67b0619b1b4d81bfe38029e3799c7f0eb86b33ca7dc4c811")),
//!         transaction_index: Some(0x0_u64),
//!         log_index: Some(0x0_u64),
//!         removed: false,
//!     },
//! ));
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
//! For example, maybe for most calls, a 2 out-of 3 strategy is good enough, but for `eth_getSlot`
//! your application requires a higher threshold and more robustness with a 3-out-of-5 :
//!
//! ```rust
//! # // TODO XC-412: Use simpler example e.g. `eth_getBalance`
//! use alloy_primitives::{address, b256, bytes};
//! use evm_rpc_client::EvmRpcClient;
//! use evm_rpc_types::{ConsensusStrategy, GetLogsRpcConfig , RpcServices};
//!
//! # use evm_rpc_types::{Hex, Hex20, Hex32, MultiRpcResult};
//! # use std::str::FromStr;
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let client = EvmRpcClient::builder_for_ic()
//! #   .with_default_stub_response(MultiRpcResult::Consistent(Ok(vec![
//! #       evm_rpc_types::LogEntry {
//! #           address: Hex20::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap(),
//! #           topics: vec![
//! #               Hex32::from_str("0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef").unwrap(),
//! #               Hex32::from_str("0x000000000000000000000000000000000004444c5dc75cb358380d2e3de08a90").unwrap(),
//! #               Hex32::from_str("0x0000000000000000000000000000000aa232009084bd71a5797d089aa4edfad4").unwrap(),
//! #           ],
//! #           data: Hex::from_str("0x00000000000000000000000000000000000000000000000000000000cd566ae8").unwrap(),
//! #           block_number: Some(0x161bd70_u64.into()),
//! #           transaction_hash: Some(Hex32::from_str("0xfe5bc88d0818b66a67b0619b1b4d81bfe38029e3799c7f0eb86b33ca7dc4c811").unwrap()),
//! #           transaction_index: Some(0x0_u64.into()),
//! #           block_hash: Some(Hex32::from_str("0x0bbd9b12140e674cdd55e63539a25df8280a70cee3676c94d8e05fa5f868a914").unwrap()),
//! #           log_index: Some(0x0_u64.into()),
//! #           removed: false,
//! #       }
//! #   ])))
//!     .with_rpc_sources(RpcServices::EthMainnet(None))
//!     .with_consensus_strategy(ConsensusStrategy::Threshold {
//!         total: Some(3),
//!         min: 2,
//!     })
//!     .build();
//!
//! let result = client
//!     .get_logs(vec![address!("0xdac17f958d2ee523a2206206994597c13d831ec7")])
//!     .with_rpc_config(GetLogsRpcConfig {
//!         response_consensus: Some(ConsensusStrategy::Threshold {
//!             total: Some(5),
//!             min: 3,
//!         }),
//!         ..Default::default()
//!     })
//!     .send()
//!     .await
//!     .expect_consistent();
//!
//! assert!(result.is_ok());
//! assert_eq!(result.unwrap().first(), Some(
//!     &alloy_rpc_types::Log {
//!         inner: alloy_primitives::Log {
//!             address: address!("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"),
//!             data: alloy_primitives::LogData::new(
//!                 vec![
//!                     b256!("0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef"),
//!                     b256!("0x000000000000000000000000000000000004444c5dc75cb358380d2e3de08a90"),
//!                     b256!("0x0000000000000000000000000000000aa232009084bd71a5797d089aa4edfad4"),
//!                 ],
//!                 bytes!("0x00000000000000000000000000000000000000000000000000000000cd566ae8"),
//!             ).unwrap(),
//!         },
//!         block_hash: Some(b256!("0x0bbd9b12140e674cdd55e63539a25df8280a70cee3676c94d8e05fa5f868a914")),
//!         block_number: Some(0x161bd70_u64),
//!         block_timestamp: None,
//!         transaction_hash: Some(b256!("0xfe5bc88d0818b66a67b0619b1b4d81bfe38029e3799c7f0eb86b33ca7dc4c811")),
//!         transaction_index: Some(0x0_u64),
//!         log_index: Some(0x0_u64),
//!         removed: false,
//!     },
//! ));
//! # Ok(())
//! # }
//! ```

#![forbid(unsafe_code)]
#![forbid(missing_docs)]

#[cfg(not(target_arch = "wasm32"))]
pub mod fixtures;
mod request;
mod runtime;

use crate::request::{Request, RequestBuilder};
use candid::{CandidType, Principal};
use evm_rpc_types::{ConsensusStrategy, GetLogsArgs, RpcConfig, RpcServices};
use ic_cdk::api::call::RejectionCode as IcCdkRejectionCode;
use ic_error_types::RejectCode;
use request::{GetLogsRequest, GetLogsRequestBuilder};
#[cfg(feature = "pocket-ic")]
pub use runtime::{
    forever, once, MockOutcall, MockOutcallBody, MockOutcallBuilder, MockOutcallQueue,
    MockOutcallRepeat, PocketIcRuntime, RepeatExt,
};
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
pub struct EvmRpcClient<R> {
    config: Arc<ClientConfig<R>>,
}

impl<R> Clone for EvmRpcClient<R> {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
        }
    }
}

impl EvmRpcClient<IcRuntime> {
    /// Creates a [`ClientBuilder`] to configure a [`EvmRpcClient`] targeting [`EVM_RPC_CANISTER`]
    /// running on the Internet Computer.
    pub fn builder_for_ic() -> ClientBuilder<IcRuntime> {
        ClientBuilder::new(IcRuntime, EVM_RPC_CANISTER)
    }
}

/// Client to interact with the EVM RPC canister.
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct ClientConfig<R> {
    runtime: R,
    evm_rpc_canister: Principal,
    rpc_config: Option<RpcConfig>,
    rpc_services: RpcServices,
}

/// A [`ClientBuilder`] to create a [`EvmRpcClient`] with custom configuration.
#[must_use]
pub struct ClientBuilder<R> {
    config: ClientConfig<R>,
}

impl<R> ClientBuilder<R> {
    fn new(runtime: R, evm_rpc_canister: Principal) -> Self {
        Self {
            config: ClientConfig {
                runtime,
                evm_rpc_canister,
                rpc_config: None,
                rpc_services: RpcServices::EthMainnet(None),
            },
        }
    }

    /// Modify the existing runtime by applying a transformation function.
    ///
    /// The transformation does not necessarily produce a runtime of the same type.
    pub fn with_runtime<S, F: FnOnce(R) -> S>(self, other_runtime: F) -> ClientBuilder<S> {
        ClientBuilder {
            config: ClientConfig {
                runtime: other_runtime(self.config.runtime),
                evm_rpc_canister: self.config.evm_rpc_canister,
                rpc_config: self.config.rpc_config,
                rpc_services: self.config.rpc_services,
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

    /// Creates a [`EvmRpcClient`] from the configuration specified in the [`ClientBuilder`].
    pub fn build(self) -> EvmRpcClient<R> {
        EvmRpcClient {
            config: Arc::new(self.config),
        }
    }
}

impl<R> EvmRpcClient<R> {
    /// Call `eth_getLogs` on the EVM RPC canister.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use alloy_primitives::{address, b256, bytes};
    /// use evm_rpc_client::EvmRpcClient;
    ///
    /// # use evm_rpc_types::{Hex, Hex20, Hex32, MultiRpcResult};
    /// # use std::str::FromStr;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = EvmRpcClient::builder_for_ic()
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
    /// let request = client.get_logs(vec![address!("0xdac17f958d2ee523a2206206994597c13d831ec7")]);
    ///
    /// let result = request
    ///     .with_cycles(10_000_000_000)
    ///     .send()
    ///     .await
    ///     .expect_consistent();
    ///
    /// assert!(result.is_ok());
    /// assert_eq!(result.unwrap().first(), Some(
    ///     &alloy_rpc_types::Log {
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
    pub fn get_logs(&self, params: impl Into<GetLogsArgs>) -> GetLogsRequestBuilder<R> {
        RequestBuilder::new(
            self.clone(),
            GetLogsRequest::new(params.into()),
            10_000_000_000,
        )
    }
}

impl<R: Runtime> EvmRpcClient<R> {
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

fn convert_reject_code(code: IcCdkRejectionCode) -> RejectCode {
    match code {
        IcCdkRejectionCode::SysFatal => RejectCode::SysFatal,
        IcCdkRejectionCode::SysTransient => RejectCode::SysTransient,
        IcCdkRejectionCode::DestinationInvalid => RejectCode::DestinationInvalid,
        IcCdkRejectionCode::CanisterReject => RejectCode::CanisterReject,
        IcCdkRejectionCode::CanisterError => RejectCode::CanisterError,
        IcCdkRejectionCode::Unknown => {
            // This can only happen if there is a new error code on ICP that the CDK is not aware of.
            // We map it to SysFatal since none of the other error codes apply.
            // In particular, note that RejectCode::SysUnknown is only applicable to inter-canister
            // calls that used ic0.call_with_best_effort_response.
            RejectCode::SysFatal
        }
        IcCdkRejectionCode::NoError => {
            unreachable!("inter-canister calls should never produce a RejectionCode::NoError error")
        }
    }
}

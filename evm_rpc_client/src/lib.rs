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
//! use evm_rpc_client::EvmRpcClient;
//!
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! # use evm_rpc_types::{GetLogsArgs, MultiRpcResult, RpcError};
//! let client = EvmRpcClient::builder_for_ic()
//! #   // TODO XC-412: Return meaningful response
//! #   .with_default_stub_response(MultiRpcResult::Consistent::<Vec<evm_rpc_types::LogEntry>>(Ok(vec![])))
//!     .build();
//!
//! // TODO XC-412: Fetch with meaningful parameters
//! let request = client.get_logs(GetLogsArgs {
//!     from_block: None,
//!     to_block: None,
//!     addresses: vec![],
//!     topics: None,
//! });
//!
//! let logs = request
//!     .with_cycles(10_000_000_000)
//!     .send()
//!     .await
//!     .expect_consistent();
//!
//! assert_eq!(logs, Ok(vec![]));
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
//! use evm_rpc_client::EvmRpcClient;
//! use evm_rpc_types::{ConsensusStrategy, GetLogsRpcConfig , RpcServices};
//!
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! # use evm_rpc_types::{GetLogsArgs, MultiRpcResult};
//! let client = EvmRpcClient::builder_for_ic()
//! #   // TODO XC-412: Return meaningful response
//! #   .with_default_stub_response(MultiRpcResult::<Vec<evm_rpc_types::LogEntry>>::Consistent(Ok(vec![])))
//!     .with_rpc_sources(RpcServices::EthMainnet(None))
//!     .with_consensus_strategy(ConsensusStrategy::Threshold {
//!         total: Some(3),
//!         min: 2,
//!     })
//!     .build();
//!
//! let logs = client
//!     // TODO XC-412: Fetch with meaningful parameters
//! .get_logs(GetLogsArgs {
//!     from_block: None,
//!     to_block: None,
//!     addresses: vec![],
//!     topics: None,
//! })
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
//! assert_eq!(logs, Ok(vec![]));
//! # Ok(())
//! # }
//! ```

#![forbid(unsafe_code)]
#![forbid(missing_docs)]

#[cfg(not(target_arch = "wasm32"))]
pub mod fixtures;
mod request;

use crate::request::{Request, RequestBuilder};
use async_trait::async_trait;
use candid::utils::ArgumentEncoder;
use candid::{CandidType, Principal};
use evm_rpc_types::{ConsensusStrategy, GetLogsArgs, RpcConfig, RpcServices};
use ic_cdk::api::call::RejectionCode as IcCdkRejectionCode;
use ic_error_types::RejectCode;
use request::{GetLogsRequest, GetLogsRequestBuilder};
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

/// Abstract the canister runtime so that the client code can be reused:
/// * in production using `ic_cdk`,
/// * in unit tests by mocking this trait,
/// * in integration tests by implementing this trait for `PocketIc`.
#[async_trait]
pub trait Runtime {
    /// Defines how asynchronous inter-canister update calls are made.
    async fn update_call<In, Out>(
        &self,
        id: Principal,
        method: &str,
        args: In,
        cycles: u128,
    ) -> Result<Out, (RejectCode, String)>
    where
        In: ArgumentEncoder + Send,
        Out: CandidType + DeserializeOwned;

    /// Defines how asynchronous inter-canister query calls are made.
    async fn query_call<In, Out>(
        &self,
        id: Principal,
        method: &str,
        args: In,
    ) -> Result<Out, (RejectCode, String)>
    where
        In: ArgumentEncoder + Send,
        Out: CandidType + DeserializeOwned;
}

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
    /// use evm_rpc_client::EvmRpcClient;
    ///
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # use evm_rpc_types::{GetLogsArgs, MultiRpcResult, RpcError};
    /// let client = EvmRpcClient::builder_for_ic()
    /// #   // TODO XC-412: Return meaningful response
    /// #   .with_default_stub_response(MultiRpcResult::Consistent::<Vec<evm_rpc_types::LogEntry>>(Ok(vec![])))
    ///     .build();
    ///
    /// // TODO XC-412: Fetch with meaningful parameters
    /// let request = client.get_logs(GetLogsArgs {
    ///     from_block: None,
    ///     to_block: None,
    ///     addresses: vec![],
    ///     topics: None,
    /// });
    ///
    /// let logs = request
    ///     .with_cycles(10_000_000_000)
    ///     .send()
    ///     .await
    ///     .expect_consistent();
    ///
    /// assert_eq!(logs, Ok(vec![]));
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

/// Runtime when interacting with a canister running on the Internet Computer.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct IcRuntime;

#[async_trait]
impl Runtime for IcRuntime {
    async fn update_call<In, Out>(
        &self,
        id: Principal,
        method: &str,
        args: In,
        cycles: u128,
    ) -> Result<Out, (RejectCode, String)>
    where
        In: ArgumentEncoder + Send,
        Out: CandidType + DeserializeOwned,
    {
        ic_cdk::api::call::call_with_payment128(id, method, args, cycles)
            .await
            .map(|(res,)| res)
            .map_err(|(code, message)| (convert_reject_code(code), message))
    }

    async fn query_call<In, Out>(
        &self,
        id: Principal,
        method: &str,
        args: In,
    ) -> Result<Out, (RejectCode, String)>
    where
        In: ArgumentEncoder + Send,
        Out: CandidType + DeserializeOwned,
    {
        ic_cdk::api::call::call(id, method, args)
            .await
            .map(|(res,)| res)
            .map_err(|(code, message)| (convert_reject_code(code), message))
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

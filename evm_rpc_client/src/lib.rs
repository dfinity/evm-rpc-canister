//! TODO XC-412: Add top-level documentation
//! TODO XC-412: Add examples (needs dummy runtime)

#![forbid(unsafe_code)]
#![forbid(missing_docs)]

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

impl<R> EvmRpcClient<R> {
    /// Creates a [`ClientBuilder`] to configure a [`EvmRpcClient`].
    pub fn builder(runtime: R, evm_rpc_canister: Principal) -> ClientBuilder<R> {
        ClientBuilder::new(runtime, evm_rpc_canister)
    }

    /// Returns a reference to the client's runtime.
    pub fn runtime(&self) -> &R {
        &self.config.runtime
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
    /// Call `get_ethLogs` on the EVM RPC canister.
    /// TODO XC-412: Add docs and examples
    pub fn get_logs(&self, params: impl Into<GetLogsArgs>) -> GetLogsRequestBuilder<R> {
        RequestBuilder::new(
            self.clone(),
            GetLogsRequest::new(params.into()),
            10_000_000_000,
        )
    }
}

impl<R: Runtime> EvmRpcClient<R> {
    /// Call `getProviders` on the EVM RPC canister.
    pub async fn get_providers(&self) -> Vec<evm_rpc_types::Provider> {
        self.config
            .runtime
            .query_call(self.config.evm_rpc_canister, "getProviders", ())
            .await
            .unwrap()
    }

    /// Call `getServiceProviderMap` on the EVM RPC canister.
    // TODO XC-412: Create type alias in `evm_rpc_types` for `ProviderId` i.e. `u64`
    pub async fn get_service_provider_map(&self) -> Vec<(evm_rpc_types::RpcService, u64)> {
        self.config
            .runtime
            .query_call(self.config.evm_rpc_canister, "getServiceProviderMap", ())
            .await
            .unwrap()
    }

    /// Call `updateApiKeys` on the EVM RPC canister.
    // TODO XC-412: Create type alias in `evm_rpc_types` for `ProviderId` i.e. `u64`
    pub async fn update_api_keys(&self, api_keys: &[(u64, Option<String>)]) {
        self.config
            .runtime
            .update_call(
                self.config.evm_rpc_canister,
                "updateApiKeys",
                (api_keys.to_vec(),),
                0,
            )
            .await
            .unwrap()
    }

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
            // In particular, note that RejectCode::SysUnknown is only applicable to inter-canister calls that used ic0.call_with_best_effort_response.
            RejectCode::SysFatal
        }
        IcCdkRejectionCode::NoError => {
            unreachable!("inter-canister calls should never produce a RejectionCode::NoError error")
        }
    }
}

use crate::{EvmRpcClient, Runtime};
use candid::CandidType;
use evm_rpc_types::{BlockTag, GetLogsArgs, GetLogsRpcConfig, Hex20, Hex32, MultiRpcResult, RpcConfig, RpcServices};
use ic_error_types::RejectCode;
use serde::de::DeserializeOwned;
use std::fmt::{Debug, Formatter};
use strum::EnumIter;

#[derive(Debug, Clone)]
pub struct GetLogsRequest(GetLogsArgs);

impl GetLogsRequest {
    pub fn new(params: GetLogsArgs) -> Self {
        Self(params)
    }
}

impl EvmRpcRequest for GetLogsRequest {
    type Config = GetLogsRpcConfig;
    type Params = GetLogsArgs;
    type CandidOutput = MultiRpcResult<Vec<evm_rpc_types::LogEntry>>;
    type Output = MultiRpcResult<Vec<alloy_rpc_types::Log>>;

    fn endpoint(&self) -> EvmRpcEndpoint {
        EvmRpcEndpoint::GetLogs
    }

    fn params(self) -> Self::Params {
        self.0
    }
}

pub type GetLogsRequestBuilder<R> = RequestBuilder<
    R,
    GetLogsRpcConfig,
    GetLogsArgs,
    MultiRpcResult<Vec<evm_rpc_types::LogEntry>>,
    MultiRpcResult<Vec<alloy_rpc_types::Log>>,
>;

impl<R> GetLogsRequestBuilder<R> {
    /// Change the `from_block` parameter for an `eth_getLogs` request.
    pub fn with_from_block(mut self, from_block: BlockTag) -> Self {
        self.request.params.from_block = Some(from_block);
        self
    }

    /// Change the `to_block` parameter for an `eth_getLogs` request.
    pub fn with_to_block(mut self, to_block: BlockTag) -> Self {
        self.request.params.to_block = Some(to_block);
        self
    }

    /// Change the `addresses` parameter for an `eth_getLogs` request.
    pub fn with_addresses(mut self, addresses: Vec<Hex20>) -> Self {
        self.request.params.addresses = addresses;
        self
    }

    /// Change the `topics` parameter for an `eth_getLogs` request.
    pub fn with_topics(mut self, topics: Vec<Vec<Hex32>>) -> Self {
        self.request.params.topics = Some(topics);
        self
    }
}

/// Ethereum RPC endpoint supported by the EVM RPC canister.
pub trait EvmRpcRequest {
    /// Type of RPC config for that request.
    type Config;
    /// The type of parameters taken by this endpoint.
    type Params;
    /// The Candid type returned when executing this request which is then converted to [`Self::Output`].
    type CandidOutput;
    /// The type returned by this endpoint.
    type Output;

    /// The name of the endpoint on the EVM RPC canister.
    fn endpoint(&self) -> EvmRpcEndpoint;

    /// Return the request parameters.
    fn params(self) -> Self::Params;
}

/// Endpoint on the EVM RPC canister triggering a call to EVM providers.
#[derive(Clone, Debug, Ord, PartialOrd, Eq, PartialEq, EnumIter)]
pub enum EvmRpcEndpoint {
    /// `eth_getLogs` endpoint.
    GetLogs,
}

impl EvmRpcEndpoint {
    /// Method name on the EVM RPC canister
    pub fn rpc_method(&self) -> &'static str {
        match &self {
            Self::GetLogs => "eth_getLogs",
        }
    }
}

/// A builder to construct a [`Request`].
///
/// To construct a [`RequestBuilder`], refer to the [`EvmRpcClient`] documentation.
#[must_use = "RequestBuilder does nothing until you 'send' it"]
pub struct RequestBuilder<Runtime, Config, Params, CandidOutput, Output> {
    client: EvmRpcClient<Runtime>,
    request: Request<Config, Params, CandidOutput, Output>,
}

impl<Runtime, Config: Clone, Params: Clone, CandidOutput, Output> Clone
    for RequestBuilder<Runtime, Config, Params, CandidOutput, Output>
{
    fn clone(&self) -> Self {
        Self {
            client: self.client.clone(),
            request: self.request.clone(),
        }
    }
}

impl<Runtime: Debug, Config: Debug, Params: Debug, CandidOutput, Output> Debug
    for RequestBuilder<Runtime, Config, Params, CandidOutput, Output>
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let RequestBuilder { client, request } = &self;
        f.debug_struct("RequestBuilder")
            .field("client", client)
            .field("request", request)
            .finish()
    }
}

impl<Runtime, Config, Params, CandidOutput, Output>
    RequestBuilder<Runtime, Config, Params, CandidOutput, Output>
{
    pub(super) fn new<RpcRequest>(
        client: EvmRpcClient<Runtime>,
        rpc_request: RpcRequest,
        cycles: u128,
    ) -> Self
    where
        RpcRequest: EvmRpcRequest<
            Config = Config,
            Params = Params,
            CandidOutput = CandidOutput,
            Output = Output,
        >,
        Config: From<RpcConfig>,
    {
        let endpoint = rpc_request.endpoint();
        let params = rpc_request.params();
        let request = Request {
            endpoint,
            rpc_services: client.config.rpc_services.clone(),
            rpc_config: client.config.rpc_config.clone().map(Config::from),
            params,
            cycles,
            _candid_marker: Default::default(),
            _output_marker: Default::default(),
        };
        RequestBuilder::<Runtime, Config, Params, CandidOutput, Output> { client, request }
    }

    /// Change the amount of cycles to send for that request.
    pub fn with_cycles(mut self, cycles: u128) -> Self {
        *self.request.cycles_mut() = cycles;
        self
    }

    /// Change the parameters to send for that request.
    pub fn with_params(mut self, params: impl Into<Params>) -> Self {
        *self.request.params_mut() = params.into();
        self
    }

    /// Modify current parameters to send for that request.
    pub fn modify_params<F>(mut self, mutator: F) -> Self
    where
        F: FnOnce(&mut Params),
    {
        mutator(self.request.params_mut());
        self
    }

    /// Change the RPC configuration to use for that request.
    pub fn with_rpc_config(mut self, rpc_config: impl Into<Config>) -> Self {
        *self.request.rpc_config_mut() = Some(rpc_config.into());
        self
    }
}

impl<R: Runtime, Config, Params, CandidOutput, Output>
    RequestBuilder<R, Config, Params, CandidOutput, Output>
{
    /// Constructs the [`Request`] and sends it using the [`EvmRpcClient`] returning the response.
    ///
    /// # Panics
    ///
    /// If the request was not successful.
    pub async fn send(self) -> Output
    where
        Config: CandidType + Send,
        Params: CandidType + Send,
        CandidOutput: Into<Output> + CandidType + DeserializeOwned,
    {
        self.client
            .execute_request::<Config, Params, CandidOutput, Output>(self.request)
            .await
    }

    /// Constructs the [`Request`] and sends it using the [`EvmRpcClient`]. This method returns
    /// either the request response or any error that occurs while sending the request.
    pub async fn try_send(self) -> Result<Output, (RejectCode, String)>
    where
        Config: CandidType + Send,
        Params: CandidType + Send,
        CandidOutput: Into<Output> + CandidType + DeserializeOwned,
    {
        self.client
            .try_execute_request::<Config, Params, CandidOutput, Output>(self.request)
            .await
    }
}

impl<Runtime, Params, CandidOutput, Output>
    RequestBuilder<Runtime, GetLogsRpcConfig, Params, CandidOutput, Output>
{
    /// Change the max block range error for `eth_getLogs` request.
    pub fn with_max_block_range(mut self, max_block_range: u32) -> Self {
        let config = self.request.rpc_config_mut().get_or_insert_default();
        config.max_block_range = Some(max_block_range);
        self
    }
}

/// A request which can be executed with `EvmRpcClient::execute_request` or `EvmRpcClient::execute_query_request`.
pub struct Request<Config, Params, CandidOutput, Output> {
    pub(super) endpoint: EvmRpcEndpoint,
    pub(super) rpc_services: RpcServices,
    pub(super) rpc_config: Option<Config>,
    pub(super) params: Params,
    pub(super) cycles: u128,
    pub(super) _candid_marker: std::marker::PhantomData<CandidOutput>,
    pub(super) _output_marker: std::marker::PhantomData<Output>,
}

impl<Config: Debug, Params: Debug, CandidOutput, Output> Debug
    for Request<Config, Params, CandidOutput, Output>
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let Request {
            endpoint,
            rpc_services,
            rpc_config,
            params,
            cycles,
            _candid_marker,
            _output_marker,
        } = &self;
        f.debug_struct("Request")
            .field("endpoint", endpoint)
            .field("rpc_services", rpc_services)
            .field("rpc_config", rpc_config)
            .field("params", params)
            .field("cycles", cycles)
            .field("_candid_marker", _candid_marker)
            .field("_output_marker", _output_marker)
            .finish()
    }
}

impl<Config: PartialEq, Params: PartialEq, CandidOutput, Output> PartialEq
    for Request<Config, Params, CandidOutput, Output>
{
    fn eq(
        &self,
        Request {
            endpoint,
            rpc_services,
            rpc_config,
            params,
            cycles,
            _candid_marker,
            _output_marker,
        }: &Self,
    ) -> bool {
        &self.endpoint == endpoint
            && &self.rpc_services == rpc_services
            && &self.rpc_config == rpc_config
            && &self.params == params
            && &self.cycles == cycles
            && &self._candid_marker == _candid_marker
            && &self._output_marker == _output_marker
    }
}

impl<Config: Clone, Params: Clone, CandidOutput, Output> Clone
    for Request<Config, Params, CandidOutput, Output>
{
    fn clone(&self) -> Self {
        Self {
            endpoint: self.endpoint.clone(),
            rpc_services: self.rpc_services.clone(),
            rpc_config: self.rpc_config.clone(),
            params: self.params.clone(),
            cycles: self.cycles,
            _candid_marker: self._candid_marker,
            _output_marker: self._output_marker,
        }
    }
}

impl<Config, Params, CandidOutput, Output> Request<Config, Params, CandidOutput, Output> {
    /// Get a mutable reference to the cycles.
    #[inline]
    pub fn cycles_mut(&mut self) -> &mut u128 {
        &mut self.cycles
    }

    /// Get a mutable reference to the RPC configuration.
    #[inline]
    pub fn rpc_config_mut(&mut self) -> &mut Option<Config> {
        &mut self.rpc_config
    }

    /// Get a mutable reference to the request parameters.
    #[inline]
    pub fn params_mut(&mut self) -> &mut Params {
        &mut self.params
    }
}

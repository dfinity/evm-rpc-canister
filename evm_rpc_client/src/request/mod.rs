use crate::{EvmRpcClient, Runtime};
use candid::CandidType;
use evm_rpc_types::{
    BlockTag, CallArgs, FeeHistoryArgs, GetLogsArgs, GetLogsRpcConfig, GetTransactionCountArgs,
    Hex, Hex20, Hex32, MultiRpcResult, Nat256, RpcConfig, RpcServices,
};
use ic_error_types::RejectCode;
use serde::de::DeserializeOwned;
use std::fmt::{Debug, Formatter};
use strum::EnumIter;

#[derive(Debug, Clone)]
pub struct CallRequest(CallArgs);

impl CallRequest {
    pub fn new(params: CallArgs) -> Self {
        Self(params)
    }
}

impl EvmRpcRequest for CallRequest {
    type Config = RpcConfig;
    type Params = CallArgs;
    type CandidOutput = MultiRpcResult<Hex>;

    fn endpoint(&self) -> EvmRpcEndpoint {
        EvmRpcEndpoint::Call
    }

    fn params(self) -> Self::Params {
        self.0
    }
}

pub type CallRequestBuilder<R> = RequestBuilder<
    R,
    <CallRequest as EvmRpcRequest>::Config,
    <CallRequest as EvmRpcRequest>::Params,
    <CallRequest as EvmRpcRequest>::CandidOutput,
>;

impl<R> CallRequestBuilder<R> {
    /// Change the `block` parameter for an `eth_call` request.
    pub fn with_block(mut self, block: impl Into<BlockTag>) -> Self {
        self.request.params.block = Some(block.into());
        self
    }
}

#[derive(Debug, Clone)]
pub struct FeeHistoryRequest(FeeHistoryArgs);

impl FeeHistoryRequest {
    pub fn new(params: FeeHistoryArgs) -> Self {
        Self(params)
    }
}

impl EvmRpcRequest for FeeHistoryRequest {
    type Config = RpcConfig;
    type Params = FeeHistoryArgs;
    type CandidOutput = MultiRpcResult<evm_rpc_types::FeeHistory>;

    fn endpoint(&self) -> EvmRpcEndpoint {
        EvmRpcEndpoint::FeeHistory
    }

    fn params(self) -> Self::Params {
        self.0
    }
}

pub type FeeHistoryRequestBuilder<R> = RequestBuilder<
    R,
    <FeeHistoryRequest as EvmRpcRequest>::Config,
    <FeeHistoryRequest as EvmRpcRequest>::Params,
    <FeeHistoryRequest as EvmRpcRequest>::CandidOutput,
>;

impl<R> FeeHistoryRequestBuilder<R> {
    /// Change the `block_count` parameter for an `eth_feeHistory` request.
    pub fn with_block_count(mut self, block_count: impl Into<Nat256>) -> Self {
        self.request.params.block_count = block_count.into();
        self
    }

    /// Change the `newest_block` parameter for an `eth_feeHistory` request.
    pub fn with_newest_block(mut self, newest_block: impl Into<BlockTag>) -> Self {
        self.request.params.newest_block = newest_block.into();
        self
    }

    /// Change the `reward_percentiles` parameter for an `eth_feeHistory` request.
    pub fn with_reward_percentiles(mut self, reward_percentiles: impl Into<Vec<u8>>) -> Self {
        self.request.params.reward_percentiles = Some(reward_percentiles.into());
        self
    }
}

#[derive(Debug, Clone)]
pub struct GetBlockByNumberRequest(BlockTag);

impl GetBlockByNumberRequest {
    pub fn new(params: BlockTag) -> Self {
        Self(params)
    }
}

impl EvmRpcRequest for GetBlockByNumberRequest {
    type Config = RpcConfig;
    type Params = BlockTag;
    type CandidOutput = MultiRpcResult<evm_rpc_types::Block>;

    fn endpoint(&self) -> EvmRpcEndpoint {
        EvmRpcEndpoint::GetBlockByNumber
    }

    fn params(self) -> Self::Params {
        self.0
    }
}

pub type GetBlockByNumberRequestBuilder<R> = RequestBuilder<
    R,
    <GetBlockByNumberRequest as EvmRpcRequest>::Config,
    <GetBlockByNumberRequest as EvmRpcRequest>::Params,
    <GetBlockByNumberRequest as EvmRpcRequest>::CandidOutput,
>;

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

    fn endpoint(&self) -> EvmRpcEndpoint {
        EvmRpcEndpoint::GetLogs
    }

    fn params(self) -> Self::Params {
        self.0
    }
}

pub type GetLogsRequestBuilder<R> = RequestBuilder<
    R,
    <GetLogsRequest as EvmRpcRequest>::Config,
    <GetLogsRequest as EvmRpcRequest>::Params,
    <GetLogsRequest as EvmRpcRequest>::CandidOutput,
>;

impl<R> GetLogsRequestBuilder<R> {
    /// Change the `from_block` parameter for an `eth_getLogs` request.
    pub fn with_from_block(mut self, from_block: impl Into<BlockTag>) -> Self {
        self.request.params.from_block = Some(from_block.into());
        self
    }

    /// Change the `to_block` parameter for an `eth_getLogs` request.
    pub fn with_to_block(mut self, to_block: impl Into<BlockTag>) -> Self {
        self.request.params.to_block = Some(to_block.into());
        self
    }

    /// Change the `addresses` parameter for an `eth_getLogs` request.
    pub fn with_addresses(mut self, addresses: Vec<impl Into<Hex20>>) -> Self {
        self.request.params.addresses = addresses.into_iter().map(Into::into).collect();
        self
    }

    /// Change the `topics` parameter for an `eth_getLogs` request.
    pub fn with_topics(mut self, topics: Vec<Vec<impl Into<Hex32>>>) -> Self {
        self.request.params.topics = Some(
            topics
                .into_iter()
                .map(|array| array.into_iter().map(Into::into).collect())
                .collect(),
        );
        self
    }
}

#[derive(Debug, Clone)]
pub struct GetTransactionCountRequest(GetTransactionCountArgs);

impl GetTransactionCountRequest {
    pub fn new(params: GetTransactionCountArgs) -> Self {
        Self(params)
    }
}

impl EvmRpcRequest for GetTransactionCountRequest {
    type Config = RpcConfig;
    type Params = GetTransactionCountArgs;
    type CandidOutput = MultiRpcResult<Nat256>;

    fn endpoint(&self) -> EvmRpcEndpoint {
        EvmRpcEndpoint::GetTransactionCount
    }

    fn params(self) -> Self::Params {
        self.0
    }
}

pub type GetTransactionCountRequestBuilder<R> = RequestBuilder<
    R,
    <GetTransactionCountRequest as EvmRpcRequest>::Config,
    <GetTransactionCountRequest as EvmRpcRequest>::Params,
    <GetTransactionCountRequest as EvmRpcRequest>::CandidOutput,
>;

impl<R> GetTransactionCountRequestBuilder<R> {
    /// Change the `address` parameter for an `eth_getTransactionCount` request.
    pub fn with_address(mut self, address: impl Into<Hex20>) -> Self {
        self.request.params.address = address.into();
        self
    }

    /// Change the `block` parameter for an `eth_getTransactionCount` request.
    pub fn with_block(mut self, block: impl Into<BlockTag>) -> Self {
        self.request.params.block = block.into();
        self
    }
}

#[derive(Debug, Clone)]
pub struct GetTransactionReceiptRequest(Hex32);

impl GetTransactionReceiptRequest {
    pub fn new(params: Hex32) -> Self {
        Self(params)
    }
}

impl EvmRpcRequest for GetTransactionReceiptRequest {
    type Config = RpcConfig;
    type Params = Hex32;
    type CandidOutput = MultiRpcResult<Option<evm_rpc_types::TransactionReceipt>>;

    fn endpoint(&self) -> EvmRpcEndpoint {
        EvmRpcEndpoint::GetTransactionReceipt
    }

    fn params(self) -> Self::Params {
        self.0
    }
}

pub type GetTransactionReceiptRequestBuilder<R> = RequestBuilder<
    R,
    <GetTransactionReceiptRequest as EvmRpcRequest>::Config,
    <GetTransactionReceiptRequest as EvmRpcRequest>::Params,
    <GetTransactionReceiptRequest as EvmRpcRequest>::CandidOutput,
>;

#[derive(Debug, Clone)]
pub struct JsonRequest(String);

impl TryFrom<serde_json::Value> for JsonRequest {
    type Error = String;

    fn try_from(value: serde_json::Value) -> Result<Self, Self::Error> {
        serde_json::to_string(&value)
            .map(JsonRequest)
            .map_err(|e| e.to_string())
    }
}

impl EvmRpcRequest for JsonRequest {
    type Config = RpcConfig;
    type Params = String;
    type CandidOutput = MultiRpcResult<String>;

    fn endpoint(&self) -> EvmRpcEndpoint {
        EvmRpcEndpoint::JsonRequest
    }

    fn params(self) -> Self::Params {
        self.0
    }
}

pub type JsonRequestBuilder<R> = RequestBuilder<
    R,
    <JsonRequest as EvmRpcRequest>::Config,
    <JsonRequest as EvmRpcRequest>::Params,
    <JsonRequest as EvmRpcRequest>::CandidOutput,
>;

#[derive(Debug, Clone)]
pub struct SendRawTransactionRequest(Hex);

impl SendRawTransactionRequest {
    pub fn new(params: Hex) -> Self {
        Self(params)
    }
}

impl EvmRpcRequest for SendRawTransactionRequest {
    type Config = RpcConfig;
    type Params = Hex;
    type CandidOutput = MultiRpcResult<evm_rpc_types::SendRawTransactionStatus>;

    fn endpoint(&self) -> EvmRpcEndpoint {
        EvmRpcEndpoint::SendRawTransaction
    }

    fn params(self) -> Self::Params {
        self.0
    }
}

pub type SendRawTransactionRequestBuilder<R> = RequestBuilder<
    R,
    <SendRawTransactionRequest as EvmRpcRequest>::Config,
    <SendRawTransactionRequest as EvmRpcRequest>::Params,
    <SendRawTransactionRequest as EvmRpcRequest>::CandidOutput,
>;

/// Ethereum RPC endpoint supported by the EVM RPC canister.
pub trait EvmRpcRequest {
    /// Type of RPC config for that request.
    type Config;
    /// The type of parameters taken by this endpoint.
    type Params;
    /// The Candid type returned when executing this request which is then converted to [`Self::Output`].
    type CandidOutput;

    /// The name of the endpoint on the EVM RPC canister.
    fn endpoint(&self) -> EvmRpcEndpoint;

    /// Return the request parameters.
    fn params(self) -> Self::Params;
}

/// Endpoint on the EVM RPC canister triggering a call to EVM providers.
#[derive(Clone, Debug, Ord, PartialOrd, Eq, PartialEq, EnumIter)]
pub enum EvmRpcEndpoint {
    /// `eth_call` endpoint.
    Call,
    /// `eth_feeHistory` endpoint.
    FeeHistory,
    /// `eth_getBlockByNumber` endpoint.
    GetBlockByNumber,
    /// `eth_getLogs` endpoint.
    GetLogs,
    /// `eth_getTransactionCount` endpoint.
    GetTransactionCount,
    /// `eth_getTransactionReceipt` endpoint.
    GetTransactionReceipt,
    /// `json_request` endpoint.
    JsonRequest,
    /// `eth_sendRawTransaction` endpoint.
    SendRawTransaction,
}

impl EvmRpcEndpoint {
    /// Method name on the EVM RPC canister
    pub fn rpc_method(&self) -> &'static str {
        match &self {
            Self::Call => "eth_call",
            Self::FeeHistory => "eth_feeHistory",
            Self::GetBlockByNumber => "eth_getBlockByNumber",
            Self::GetLogs => "eth_getLogs",
            Self::GetTransactionCount => "eth_getTransactionCount",
            Self::GetTransactionReceipt => "eth_getTransactionReceipt",
            Self::JsonRequest => "json_request",
            Self::SendRawTransaction => "eth_sendRawTransaction",
        }
    }
}

/// A builder to construct a [`Request`].
///
/// To construct a [`RequestBuilder`], refer to the [`EvmRpcClient`] documentation.
#[must_use = "RequestBuilder does nothing until you 'send' it"]
pub struct RequestBuilder<Runtime, Config, Params, CandidOutput> {
    client: EvmRpcClient<Runtime>,
    request: Request<Config, Params, CandidOutput>,
}

impl<Runtime, Config: Clone, Params: Clone, CandidOutput> Clone
    for RequestBuilder<Runtime, Config, Params, CandidOutput>
{
    fn clone(&self) -> Self {
        Self {
            client: self.client.clone(),
            request: self.request.clone(),
        }
    }
}

impl<Runtime: Debug, Config: Debug, Params: Debug, CandidOutput> Debug
    for RequestBuilder<Runtime, Config, Params, CandidOutput>
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let RequestBuilder { client, request } = &self;
        f.debug_struct("RequestBuilder")
            .field("client", client)
            .field("request", request)
            .finish()
    }
}

impl<Runtime, Config, Params, CandidOutput>
    RequestBuilder<Runtime, Config, Params, CandidOutput>
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
        };
        RequestBuilder::<Runtime, Config, Params, CandidOutput> { client, request }
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

impl<R: Runtime, Config, Params, CandidOutput>
    RequestBuilder<R, Config, Params, CandidOutput>
{
    /// Constructs the [`Request`] and sends it using the [`EvmRpcClient`] returning the response.
    ///
    /// # Panics
    ///
    /// If the request was not successful.
    pub async fn send<Output>(self) -> Output
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
    pub async fn try_send<Output>(self) -> Result<Output, (RejectCode, String)>
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

impl<Runtime, Params, CandidOutput>
    RequestBuilder<Runtime, GetLogsRpcConfig, Params, CandidOutput>
{
    /// Change the max block range error for `eth_getLogs` request.
    pub fn with_max_block_range(mut self, max_block_range: u32) -> Self {
        let config = self.request.rpc_config_mut().get_or_insert_default();
        config.max_block_range = Some(max_block_range);
        self
    }
}

/// A request which can be executed with `EvmRpcClient::execute_request` or `EvmRpcClient::execute_query_request`.
pub struct Request<Config, Params, CandidOutput> {
    pub(super) endpoint: EvmRpcEndpoint,
    pub(super) rpc_services: RpcServices,
    pub(super) rpc_config: Option<Config>,
    pub(super) params: Params,
    pub(super) cycles: u128,
    pub(super) _candid_marker: std::marker::PhantomData<CandidOutput>,
}

impl<Config: Debug, Params: Debug, CandidOutput> Debug
    for Request<Config, Params, CandidOutput>
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let Request {
            endpoint,
            rpc_services,
            rpc_config,
            params,
            cycles,
            _candid_marker,
        } = &self;
        f.debug_struct("Request")
            .field("endpoint", endpoint)
            .field("rpc_services", rpc_services)
            .field("rpc_config", rpc_config)
            .field("params", params)
            .field("cycles", cycles)
            .field("_candid_marker", _candid_marker)
            .finish()
    }
}

impl<Config: PartialEq, Params: PartialEq, CandidOutput> PartialEq
    for Request<Config, Params, CandidOutput>
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
        }: &Self,
    ) -> bool {
        &self.endpoint == endpoint
            && &self.rpc_services == rpc_services
            && &self.rpc_config == rpc_config
            && &self.params == params
            && &self.cycles == cycles
            && &self._candid_marker == _candid_marker
    }
}

impl<Config: Clone, Params: Clone, CandidOutput> Clone
    for Request<Config, Params, CandidOutput>
{
    fn clone(&self) -> Self {
        Self {
            endpoint: self.endpoint.clone(),
            rpc_services: self.rpc_services.clone(),
            rpc_config: self.rpc_config.clone(),
            params: self.params.clone(),
            cycles: self.cycles,
            _candid_marker: self._candid_marker,
        }
    }
}

impl<Config, Params, CandidOutput> Request<Config, Params, CandidOutput> {
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

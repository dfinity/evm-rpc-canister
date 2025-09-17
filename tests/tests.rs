mod mock;
mod mock_http_runtime;
mod setup;

use crate::mock_http_runtime::mock::CanisterHttpReject;
use crate::{
    mock_http_runtime::mock::{
        json::{JsonRpcRequestMatcher, JsonRpcResponse},
        CanisterHttpReply, MockHttpOutcalls, MockHttpOutcallsBuilder,
    },
    setup::EvmRpcNonblockingSetup,
};
use alloy_primitives::{address, b256, bloom, bytes, U256};
use alloy_rpc_types::{BlockNumberOrTag, BlockTransactions};
use assert_matches::assert_matches;
use candid::{CandidType, Decode, Encode, Nat, Principal};
use canlog::{Log, LogEntry};
use evm_rpc::constants::DEFAULT_MAX_RESPONSE_BYTES;
use evm_rpc::logs::Priority;
use evm_rpc::{
    constants::{CONTENT_TYPE_HEADER_LOWERCASE, CONTENT_TYPE_VALUE},
    providers::PROVIDERS,
    types::{Metrics, ProviderId, RpcAccess, RpcMethod},
};
use evm_rpc_types::{
    BlockTag, ConsensusStrategy, EthMainnetService, EthSepoliaService, GetLogsRpcConfig, Hex,
    Hex20, Hex32, HttpOutcallError, InstallArgs, JsonRpcError, LegacyRejectionCode, MultiRpcResult,
    Nat256, Provider, ProviderError, RpcApi, RpcError, RpcResult, RpcService, RpcServices,
    ValidationError,
};
use ic_cdk::api::management_canister::main::CanisterId;
use ic_error_types::RejectCode;
use ic_http_types::{HttpRequest, HttpResponse};
use ic_management_canister_types::{CanisterSettings, HttpHeader};
use ic_test_utilities_load_wasm::load_wasm;
use maplit::hashmap;
use mock::{MockOutcall, MockOutcallBuilder};
use pocket_ic::common::rest::{
    CanisterHttpMethod, CanisterHttpResponse, MockCanisterHttpResponse, RawMessageId,
};
use pocket_ic::{ErrorCode, PocketIc, PocketIcBuilder, RejectResponse};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use std::{iter, marker::PhantomData, str::FromStr, sync::Arc, time::Duration};

const DEFAULT_CALLER_TEST_ID: Principal = Principal::from_slice(&[0x9d, 0xf7, 0x01]);
const DEFAULT_CONTROLLER_TEST_ID: Principal = Principal::from_slice(&[0x9d, 0xf7, 0x02]);
const ADDITIONAL_TEST_ID: Principal = Principal::from_slice(&[0x9d, 0xf7, 0x03]);

const INITIAL_CYCLES: u128 = 100_000_000_000_000_000;

const MAX_TICKS: usize = 10;

const MOCK_REQUEST_URL: &str = "https://cloudflare-eth.com";
const MOCK_REQUEST_PAYLOAD: &str = r#"{"id":1,"jsonrpc":"2.0","method":"eth_gasPrice"}"#;
const MOCK_REQUEST_RESPONSE: &str = r#"{"jsonrpc":"2.0","id":1,"result":"0x00112233"}"#;
const MOCK_REQUEST_RESPONSE_BYTES: u64 = 1000;
const MOCK_API_KEY: &str = "mock-api-key";

const MOCK_TRANSACTION: &str = "0xf86c098504a817c800825208943535353535353535353535353535353535353535880de0b6b3a76400008025a028ef61340bd939bc2195fe537567866003e1a15d3c71ff63e1590620aa636276a067cbe9d8997f761aecb703304b3800ccf555c9f3dc64214b297fb1966a3b6d83";
const MOCK_TRANSACTION_HASH: &str =
    "0x33469b22e9f636356c4160a87eb19df52b7412e8eac32a4a55ffe88ea8350788";

const RPC_SERVICES: &[RpcServices] = &[
    RpcServices::EthMainnet(None),
    RpcServices::EthSepolia(None),
    RpcServices::ArbitrumOne(None),
    RpcServices::BaseMainnet(None),
    RpcServices::OptimismMainnet(None),
];

const ANKR_HOSTNAME: &str = "rpc.ankr.com";
const ALCHEMY_ETH_MAINNET_HOSTNAME: &str = "eth-mainnet.g.alchemy.com";
const CLOUDFLARE_HOSTNAME: &str = "cloudflare-eth.com";
const BLOCKPI_ETH_HOSTNAME: &str = "ethereum.blockpi.network";
const BLOCKPI_ETH_SEPOLIA_HOSTNAME: &str = "ethereum-sepolia.blockpi.network";
const PUBLICNODE_ETH_MAINNET_HOSTNAME: &str = "ethereum-rpc.publicnode.com";

fn evm_rpc_wasm() -> Vec<u8> {
    load_wasm(std::env::var("CARGO_MANIFEST_DIR").unwrap(), "evm_rpc", &[])
}

fn assert_reply(result: Result<Vec<u8>, RejectResponse>) -> Vec<u8> {
    result.unwrap_or_else(|e| panic!("Expected a successful reply, got error {e}"))
}

#[derive(Clone)]
pub struct EvmRpcSetup {
    pub env: Arc<PocketIc>,
    pub caller: Principal,
    pub controller: Principal,
    pub canister_id: CanisterId,
}

impl Default for EvmRpcSetup {
    fn default() -> Self {
        Self::new()
    }
}

impl EvmRpcSetup {
    pub fn new() -> Self {
        Self::with_args(InstallArgs {
            demo: Some(true),
            ..Default::default()
        })
    }

    pub fn with_args(args: InstallArgs) -> Self {
        // The `with_fiduciary_subnet` setup below requires that `nodes_in_subnet`
        // setting (part of InstallArgs) to be set appropriately. Otherwise
        // http outcall will fail due to insufficient cycles, even when `demo` is
        // enabled (which is the default above).
        //
        // As of writing, the default value of `nodes_in_subnet` is 34, which is
        // also the node count in fiduciary subnet.
        let pocket_ic = PocketIcBuilder::new().with_fiduciary_subnet().build();
        let env = Arc::new(pocket_ic);

        let controller = DEFAULT_CONTROLLER_TEST_ID;
        let canister_id = env.create_canister_with_settings(
            None,
            Some(CanisterSettings {
                controllers: Some(vec![controller]),
                ..CanisterSettings::default()
            }),
        );
        env.add_cycles(canister_id, INITIAL_CYCLES);
        env.install_canister(
            canister_id,
            evm_rpc_wasm(),
            Encode!(&args).unwrap(),
            Some(controller),
        );

        let caller = DEFAULT_CALLER_TEST_ID;

        Self {
            env,
            caller,
            controller,
            canister_id,
        }
    }

    pub fn upgrade_canister(&self, args: InstallArgs) {
        for _ in 0..10 {
            self.env.tick();
            // Avoid `CanisterInstallCodeRateLimited` error
            self.env.advance_time(Duration::from_secs(600));
            self.env.tick();
            match self.env.upgrade_canister(
                self.canister_id,
                evm_rpc_wasm(),
                Encode!(&args).unwrap(),
                Some(self.controller),
            ) {
                Ok(_) => return,
                Err(e) if e.error_code == ErrorCode::CanisterInstallCodeRateLimited => continue,
                Err(e) => panic!("Error while upgrading canister: {e:?}"),
            }
        }
        panic!("Failed to upgrade canister after many trials!")
    }

    /// Shorthand for deriving an `EvmRpcSetup` with the caller as the canister controller.
    pub fn as_controller(mut self) -> Self {
        self.caller = self.controller;
        self
    }

    /// Shorthand for deriving an `EvmRpcSetup` with an arbitrary caller.
    pub fn as_caller<T: Into<Principal>>(mut self, id: T) -> Self {
        self.caller = id.into();
        self
    }

    fn call_update<R: CandidType + DeserializeOwned>(
        &self,
        method: &str,
        input: Vec<u8>,
    ) -> CallFlow<R> {
        CallFlow::from_update(self.clone(), method, input)
    }

    fn call_query<R: CandidType + DeserializeOwned>(&self, method: &str, input: Vec<u8>) -> R {
        let candid =
            &assert_reply(
                self.env
                    .query_call(self.canister_id, self.caller, method, input),
            );
        Decode!(candid, R).expect("error while decoding Candid response from query call")
    }

    pub fn tick_until_http_request(&self) {
        for _ in 0..MAX_TICKS {
            if !self.env.get_canister_http().is_empty() {
                break;
            }
            self.env.tick();
            self.env.advance_time(Duration::from_nanos(1));
        }
    }

    pub fn get_metrics(&self) -> Metrics {
        self.call_query("getMetrics", Encode!().unwrap())
    }

    pub fn get_service_provider_map(&self) -> Vec<(RpcService, ProviderId)> {
        self.call_query("getServiceProviderMap", Encode!().unwrap())
    }

    pub fn get_providers(&self) -> Vec<Provider> {
        self.call_query("getProviders", Encode!().unwrap())
    }

    pub fn get_nodes_in_subnet(&self) -> u32 {
        self.call_query("getNodesInSubnet", Encode!().unwrap())
    }

    pub fn request_cost(
        &self,
        source: RpcService,
        json_rpc_payload: &str,
        max_response_bytes: u64,
    ) -> RpcResult<Nat> {
        self.call_query(
            "requestCost",
            Encode!(&source, &json_rpc_payload, &max_response_bytes).unwrap(),
        )
    }

    pub fn request(
        &self,
        source: RpcService,
        json_rpc_payload: &str,
        max_response_bytes: u64,
    ) -> CallFlow<RpcResult<String>> {
        self.call_update(
            "request",
            Encode!(&source, &json_rpc_payload, &max_response_bytes).unwrap(),
        )
    }

    pub fn eth_get_transaction_receipt(
        &self,
        source: RpcServices,
        config: Option<evm_rpc_types::RpcConfig>,
        tx_hash: &str,
    ) -> CallFlow<MultiRpcResult<Option<evm_rpc_types::TransactionReceipt>>> {
        self.call_update(
            "eth_getTransactionReceipt",
            Encode!(&source, &config, &tx_hash).unwrap(),
        )
    }

    pub fn eth_send_raw_transaction(
        &self,
        source: RpcServices,
        config: Option<evm_rpc_types::RpcConfig>,
        signed_raw_transaction_hex: &str,
    ) -> CallFlow<MultiRpcResult<evm_rpc_types::SendRawTransactionStatus>> {
        let signed_raw_transaction_hex: Hex = signed_raw_transaction_hex.parse().unwrap();
        self.call_update(
            "eth_sendRawTransaction",
            Encode!(&source, &config, &signed_raw_transaction_hex).unwrap(),
        )
    }

    pub fn update_api_keys(&self, api_keys: &[(ProviderId, Option<String>)]) {
        self.call_update("updateApiKeys", Encode!(&api_keys).unwrap())
            .wait()
    }

    pub fn mock_api_keys(self) -> Self {
        self.clone().as_controller().update_api_keys(
            &PROVIDERS
                .iter()
                .filter_map(|provider| {
                    Some((
                        provider.provider_id,
                        match provider.access {
                            RpcAccess::Authenticated { .. } => Some(MOCK_API_KEY.to_string()),
                            RpcAccess::Unauthenticated { .. } => None?,
                        },
                    ))
                })
                .collect::<Vec<_>>(),
        );
        self
    }

    pub fn http_get_logs(&self, priority: &str) -> Vec<LogEntry<Priority>> {
        let request = HttpRequest {
            method: "".to_string(),
            url: format!("/logs?priority={priority}"),
            headers: vec![],
            body: serde_bytes::ByteBuf::new(),
        };
        let response = Decode!(
            &assert_reply(self.env.query_call(
                self.canister_id,
                Principal::anonymous(),
                "http_request",
                Encode!(&request).unwrap()
            )),
            HttpResponse
        )
        .unwrap();
        serde_json::from_slice::<Log<Priority>>(&response.body)
            .expect("failed to parse EVM_RPC minter log")
            .entries
    }
}

pub struct CallFlow<R> {
    setup: EvmRpcSetup,
    method: String,
    message_id: RawMessageId,
    phantom: PhantomData<R>,
}

impl<R: CandidType + DeserializeOwned> CallFlow<R> {
    pub fn from_update(setup: EvmRpcSetup, method: &str, input: Vec<u8>) -> Self {
        let message_id = setup
            .env
            .submit_call(setup.canister_id, setup.caller, method, input)
            .expect("failed to submit call");
        CallFlow::new(setup, method, message_id)
    }

    pub fn new(setup: EvmRpcSetup, method: impl ToString, message_id: RawMessageId) -> Self {
        Self {
            setup,
            method: method.to_string(),
            message_id,
            phantom: Default::default(),
        }
    }

    pub fn mock_http(self, mock: impl Into<MockOutcall>) -> Self {
        let mock = mock.into();
        self.mock_http_once_inner(&mock);
        loop {
            if !self.try_mock_http_inner(&mock) {
                break;
            }
        }
        self
    }

    pub fn mock_http_n_times(self, mock: impl Into<MockOutcall>, count: u32) -> Self {
        let mock = mock.into();
        for _ in 0..count {
            self.mock_http_once_inner(&mock);
        }
        self
    }

    pub fn mock_http_once(self, mock: impl Into<MockOutcall>) -> Self {
        let mock = mock.into();
        self.mock_http_once_inner(&mock);
        self
    }

    fn mock_http_once_inner(&self, mock: &MockOutcall) {
        if !self.try_mock_http_inner(mock) {
            panic!("no pending HTTP request for {}", self.method)
        }
    }

    fn try_mock_http_inner(&self, mock: &MockOutcall) -> bool {
        if self.setup.env.get_canister_http().is_empty() {
            self.setup.tick_until_http_request();
        }
        let http_requests = self.setup.env.get_canister_http();
        let request = match http_requests.first() {
            Some(request) => request,
            None => return false,
        };
        mock.assert_matches(request);

        let response = match mock.response.clone() {
            CanisterHttpResponse::CanisterHttpReply(reply) => {
                let max_response_bytes = request
                    .max_response_bytes
                    .unwrap_or(DEFAULT_MAX_RESPONSE_BYTES);
                if reply.body.len() as u64 > max_response_bytes {
                    //approximate replica behaviour since headers are not accounted for.
                    CanisterHttpResponse::CanisterHttpReject(
                        pocket_ic::common::rest::CanisterHttpReject {
                            reject_code: 1, //SYS_FATAL
                            message: format!(
                                "Http body exceeds size limit of {} bytes.",
                                max_response_bytes
                            ),
                        },
                    )
                } else {
                    CanisterHttpResponse::CanisterHttpReply(reply)
                }
            }
            CanisterHttpResponse::CanisterHttpReject(reject) => {
                CanisterHttpResponse::CanisterHttpReject(reject)
            }
        };
        let mock_response = MockCanisterHttpResponse {
            subnet_id: request.subnet_id,
            request_id: request.request_id,
            response,
            additional_responses: vec![],
        };
        self.setup.env.mock_canister_http_response(mock_response);
        true
    }

    pub fn wait(self) -> R {
        let candid = &assert_reply(self.setup.env.await_call(self.message_id));
        Decode!(candid, R).expect("error while decoding Candid response from update call")
    }
}

fn mock_request(builder_fn: impl Fn(MockOutcallBuilder) -> MockOutcallBuilder) {
    let setup = EvmRpcSetup::new();
    assert_matches!(
        setup
            .request(
                RpcService::Custom(RpcApi {
                    url: MOCK_REQUEST_URL.to_string(),
                    headers: Some(vec![HttpHeader {
                        name: "Custom".to_string(),
                        value: "Value".to_string(),
                    }]),
                }),
                MOCK_REQUEST_PAYLOAD,
                MOCK_REQUEST_RESPONSE_BYTES,
            )
            .mock_http(builder_fn(MockOutcallBuilder::new(
                200,
                MOCK_REQUEST_RESPONSE
            )))
            .wait(),
        Ok(_)
    );
}

#[test]
fn mock_request_should_succeed() {
    mock_request(|builder| builder)
}

#[test]
fn mock_request_should_succeed_with_url() {
    mock_request(|builder| builder.with_url(MOCK_REQUEST_URL))
}

#[test]
fn mock_request_should_succeed_with_method() {
    mock_request(|builder| builder.with_method(CanisterHttpMethod::POST))
}

#[test]
fn mock_request_should_succeed_with_request_headers() {
    mock_request(|builder| {
        builder.with_request_headers(vec![
            (CONTENT_TYPE_HEADER_LOWERCASE, CONTENT_TYPE_VALUE),
            ("Custom", "Value"),
        ])
    })
}

#[test]
fn mock_request_should_succeed_with_request_body() {
    mock_request(|builder| builder.with_raw_request_body(MOCK_REQUEST_PAYLOAD))
}

#[test]
fn mock_request_should_succeed_with_max_response_bytes() {
    mock_request(|builder| builder.with_max_response_bytes(MOCK_REQUEST_RESPONSE_BYTES))
}

#[test]
fn mock_request_should_succeed_with_all() {
    mock_request(|builder| {
        builder
            .with_url(MOCK_REQUEST_URL)
            .with_method(CanisterHttpMethod::POST)
            .with_request_headers(vec![
                (CONTENT_TYPE_HEADER_LOWERCASE, CONTENT_TYPE_VALUE),
                ("Custom", "Value"),
            ])
            .with_raw_request_body(MOCK_REQUEST_PAYLOAD)
    })
}

#[test]
#[should_panic(expected = "assertion `left == right` failed")]
fn mock_request_should_fail_with_url() {
    mock_request(|builder| builder.with_url("https://not-the-url.com"))
}

#[test]
#[should_panic(expected = "assertion `left == right` failed")]
fn mock_request_should_fail_with_method() {
    mock_request(|builder| builder.with_method(CanisterHttpMethod::GET))
}

#[test]
#[should_panic(expected = "assertion `left == right` failed")]
fn mock_request_should_fail_with_request_headers() {
    mock_request(|builder| builder.with_request_headers(vec![("Custom", "NotValue")]))
}

#[test]
#[should_panic(expected = "assertion `left == right` failed")]
fn mock_request_should_fail_with_request_body() {
    mock_request(|builder| {
        builder.with_raw_request_body(r#"{"id":1,"jsonrpc":"2.0","method":"unknown_method"}"#)
    })
}

#[test]
fn should_canonicalize_json_response() {
    let setup = EvmRpcSetup::new();
    let responses = [
        r#"{"id":1,"jsonrpc":"2.0","result":"0x00112233"}"#,
        r#"{"result":"0x00112233","id":1,"jsonrpc":"2.0"}"#,
        r#"{"result":"0x00112233","jsonrpc":"2.0","id":1}"#,
    ]
    .into_iter()
    .map(|response| {
        setup
            .request(
                RpcService::Custom(RpcApi {
                    url: MOCK_REQUEST_URL.to_string(),
                    headers: None,
                }),
                MOCK_REQUEST_PAYLOAD,
                MOCK_REQUEST_RESPONSE_BYTES,
            )
            .mock_http(MockOutcallBuilder::new(200, response))
            .wait()
    })
    .collect::<Vec<_>>();
    assert!(responses.windows(2).all(|w| w[0] == w[1]));
}

#[test]
fn should_not_modify_json_rpc_request_from_request_endpoint() {
    let setup = EvmRpcSetup::new();

    let json_rpc_request = r#"{"id":123,"jsonrpc":"2.0","method":"eth_gasPrice"}"#;
    let mock_response = r#"{"jsonrpc":"2.0","id":123,"result":"0x00112233"}"#;

    let response = setup
        .request(
            RpcService::Custom(RpcApi {
                url: MOCK_REQUEST_URL.to_string(),
                headers: None,
            }),
            json_rpc_request,
            MOCK_REQUEST_RESPONSE_BYTES,
        )
        .mock_http_once(
            MockOutcallBuilder::new(200, mock_response).with_raw_request_body(json_rpc_request),
        )
        .wait()
        .unwrap();

    assert_eq!(response, mock_response);
}

#[test]
fn should_decode_renamed_field() {
    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, CandidType)]
    pub struct Struct {
        #[serde(rename = "fieldName")]
        pub field_name: u64,
    }
    let value = Struct { field_name: 123 };
    assert_eq!(Decode!(&Encode!(&value).unwrap(), Struct).unwrap(), value);
}

#[test]
fn should_decode_checked_amount() {
    let value = Nat256::from(123_u32);
    assert_eq!(Decode!(&Encode!(&value).unwrap(), Nat256).unwrap(), value);
}

#[test]
fn should_decode_address() {
    let value = Hex20::from_str("0xdAC17F958D2ee523a2206206994597C13D831ec7").unwrap();
    assert_eq!(Decode!(&Encode!(&value).unwrap(), Hex20).unwrap(), value);
}

#[test]
fn should_decode_transaction_receipt() {
    let value = evm_rpc_types::TransactionReceipt {
        status: Some(0x1_u8.into()),
        root: None,
        transaction_hash: "0xdd5d4b18923d7aae953c7996d791118102e889bea37b48a651157a4890e4746f"
            .parse()
            .unwrap(),
        contract_address: None,
        block_number: 18_515_371_u64.into(),
        block_hash: "0x5115c07eb1f20a9d6410db0916ed3df626cfdab161d3904f45c8c8b65c90d0be"
            .parse()
            .unwrap(),
        effective_gas_price: 26_776_497_782_u64.into(),
        gas_used: 32_137_u32.into(),
        from: "0x0aa8ebb6ad5a8e499e550ae2c461197624c6e667"
            .parse()
            .unwrap(),
        logs: vec![],
        logs_bloom: "0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000".parse().unwrap(),
        to: Some("0x356cfd6e6d0000400000003900b415f80669009e"
            .parse()
            .unwrap()),
        transaction_index: 0xd9_u16.into(),
        tx_type: "0x2".parse().unwrap(),
        cumulative_gas_used: 0xf02aed_u64.into(),
    };
    assert_eq!(
        Decode!(&Encode!(&value).unwrap(), evm_rpc_types::TransactionReceipt).unwrap(),
        value
    );
}

#[tokio::test]
async fn eth_get_logs_should_succeed() {
    fn mock_request(
        from_block: BlockNumberOrTag,
        to_block: BlockNumberOrTag,
    ) -> JsonRpcRequestMatcher {
        JsonRpcRequestMatcher::with_method("eth_getLogs").with_params(json!([{
            "address" : ["0xdac17f958d2ee523a2206206994597c13d831ec7"],
            "fromBlock" : from_block,
            "toBlock" : to_block,
        }]))
    }

    fn mock_response() -> JsonRpcResponse {
        JsonRpcResponse::from(json!({
            "id" : 0,
            "jsonrpc" : "2.0",
            "result" : [
                {
                    "address" : "0xdac17f958d2ee523a2206206994597c13d831ec7",
                    "topics" : [
                        "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef",
                        "0x000000000000000000000000a9d1e08c7793af67e9d92fe308d5697fb81d3e43",
                        "0x00000000000000000000000078cccfb3d517cd4ed6d045e263e134712288ace2"
                    ],
                    "data" : "0x000000000000000000000000000000000000000000000000000000003b9c6433",
                    "blockNumber" : "0x11dc77e",
                    "transactionHash" : "0xf3ed91a03ddf964281ac7a24351573efd535b80fc460a5c2ad2b9d23153ec678",
                    "transactionIndex" : "0x65",
                    "blockHash" : "0xd5c72ad752b2f0144a878594faf8bd9f570f2f72af8e7f0940d3545a6388f629",
                    "logIndex" : "0xe8",
                    "removed" : false
                }
            ]
        }))
    }

    fn expected_logs() -> Vec<alloy_rpc_types::Log> {
        vec![alloy_rpc_types::Log {
            inner: alloy_primitives::Log::new(
                address!("0xdac17f958d2ee523a2206206994597c13d831ec7"),
                vec![
                    b256!("0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef"),
                    b256!("0x000000000000000000000000a9d1e08c7793af67e9d92fe308d5697fb81d3e43"),
                    b256!("0x00000000000000000000000078cccfb3d517cd4ed6d045e263e134712288ace2"),
                ],
                bytes!("0x000000000000000000000000000000000000000000000000000000003b9c6433"),
            )
            .unwrap(),
            block_number: Some(0x11dc77e_u64),
            transaction_hash: Some(b256!(
                "0xf3ed91a03ddf964281ac7a24351573efd535b80fc460a5c2ad2b9d23153ec678"
            )),
            transaction_index: Some(0x65_u64),
            block_hash: Some(b256!(
                "0xd5c72ad752b2f0144a878594faf8bd9f570f2f72af8e7f0940d3545a6388f629"
            )),
            log_index: Some(0xe8_u64),
            removed: false,
            block_timestamp: None,
        }]
    }
    let setup = EvmRpcNonblockingSetup::new().await.mock_api_keys().await;
    let mut offsets = (0_u64..).step_by(3);
    for source in RPC_SERVICES {
        for (config, from_block, to_block) in [
            // default block range
            (
                GetLogsRpcConfig::default(),
                BlockNumberOrTag::Number(0_u8.into()),
                BlockNumberOrTag::Number(500_u16.into()),
            ),
            // large block range
            (
                GetLogsRpcConfig {
                    max_block_range: Some(1_000),
                    ..Default::default()
                },
                BlockNumberOrTag::Number(0_u8.into()),
                BlockNumberOrTag::Number(501_u16.into()),
            ),
        ] {
            let offset = offsets.next().unwrap();
            let mocks = MockHttpOutcallsBuilder::new()
                .given(mock_request(from_block, to_block).with_id(offset))
                .respond_with(mock_response().with_id(offset))
                .given(mock_request(from_block, to_block).with_id(1 + offset))
                .respond_with(mock_response().with_id(1 + offset))
                .given(mock_request(from_block, to_block).with_id(2 + offset))
                .respond_with(mock_response().with_id(2 + offset));

            let response = setup
                .client(mocks)
                .with_rpc_sources(source.clone())
                .build()
                .get_logs(vec![address!("0xdac17f958d2ee523a2206206994597c13d831ec7")])
                .with_from_block(from_block)
                .with_to_block(to_block)
                .with_rpc_config(config)
                .send()
                .await
                .expect_consistent()
                .unwrap();

            assert_eq!(response, expected_logs());
        }
    }
}

#[tokio::test]
async fn eth_get_logs_should_fail_when_block_range_too_large() {
    let setup = EvmRpcNonblockingSetup::new().await.mock_api_keys().await;
    let error_msg_regex =
        regex::Regex::new("Requested [0-9_]+ blocks; limited to [0-9_]+").unwrap();

    for source in RPC_SERVICES {
        for (config, from_block, to_block) in [
            // default block range
            (
                GetLogsRpcConfig::default(),
                BlockTag::Number(0_u8.into()),
                BlockTag::Number(501_u16.into()),
            ),
            // large block range
            (
                GetLogsRpcConfig {
                    max_block_range: Some(1_000),
                    ..Default::default()
                },
                BlockTag::Number(0_u8.into()),
                BlockTag::Number(1001_u16.into()),
            ),
        ] {
            let client = setup
                .client(MockHttpOutcalls::NEVER)
                .with_rpc_sources(source.clone())
                .build();

            let response = client
                .get_logs(vec![address!("0xdAC17F958D2ee523a2206206994597C13D831ec7")])
                .with_from_block(from_block)
                .with_to_block(to_block)
                .with_rpc_config(config)
                .send()
                .await
                .expect_consistent()
                .unwrap_err();

            assert_matches!(
                response,
                RpcError::ValidationError(ValidationError::Custom(s)) if error_msg_regex.is_match(&s)
            )
        }
    }
}

#[tokio::test]
async fn eth_get_block_by_number_should_succeed() {
    fn mock_request() -> JsonRpcRequestMatcher {
        JsonRpcRequestMatcher::with_method("eth_getBlockByNumber")
            .with_params(json!(["latest", false]))
    }

    fn mock_response() -> JsonRpcResponse {
        JsonRpcResponse::from(json!({
            "jsonrpc": "2.0",
            "result": {
                "baseFeePerGas": "0xd7232aa34",
                "difficulty": "0x0",
                "extraData": "0x546974616e2028746974616e6275696c6465722e78797a29",
                "gasLimit": "0x1c9c380",
                "gasUsed": "0xa768c4",
                "hash": "0xc3674be7b9d95580d7f23c03d32e946f2b453679ee6505e3a778f003c5a3cfae",
                "logsBloom": "0x3e6b8420e1a13038902c24d6c2a9720a7ad4860cdc870cd5c0490011e43631134f608935bd83171247407da2c15d85014f9984608c03684c74aad48b20bc24022134cdca5f2e9d2dee3b502a8ccd39eff8040b1d96601c460e119c408c620b44fa14053013220847045556ea70484e67ec012c322830cf56ef75e09bd0db28a00f238adfa587c9f80d7e30d3aba2863e63a5cad78954555966b1055a4936643366a0bb0b1bac68d0e6267fc5bf8304d404b0c69041125219aa70562e6a5a6362331a414a96d0716990a10161b87dd9568046a742d4280014975e232b6001a0360970e569d54404b27807d7a44c949ac507879d9d41ec8842122da6772101bc8b",
                "miner": "0x388c818ca8b9251b393131c08a736a67ccb19297",
                "mixHash": "0x516a58424d4883a3614da00a9c6f18cd5cd54335a08388229a993a8ecf05042f",
                "nonce": "0x0000000000000000",
                "number": "0x11db01d",
                "parentHash": "0x43325027f6adf9befb223f8ae80db057daddcd7b48e41f60cd94bfa8877181ae",
                "receiptsRoot": "0x66934c3fd9c547036fe0e56ad01bc43c84b170be7c4030a86805ddcdab149929",
                "sha3Uncles": "0x1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347",
                "size": "0xcd35",
                "stateRoot": "0x13552447dd62f11ad885f21a583c4fa34144efe923c7e35fb018d6710f06b2b6",
                "timestamp": "0x656f96f3",
                "withdrawalsRoot": "0xecae44b2c53871003c5cc75285995764034c9b5978a904229d36c1280b141d48",
                "transactionsRoot": "0x93a1ad3d067009259b508cc95fde63b5efd7e9d8b55754314c173fdde8c0826a",
            },
            "id": 0
        }))
    }

    let setup = EvmRpcNonblockingSetup::new().await.mock_api_keys().await;

    for (source, offset) in iter::zip(RPC_SERVICES, (0_u64..).step_by(3)) {
        let mocks = MockHttpOutcallsBuilder::new()
            .given(mock_request().with_id(offset))
            .respond_with(mock_response().with_id(offset))
            .given(mock_request().with_id(1 + offset))
            .respond_with(mock_response().with_id(1 + offset))
            .given(mock_request().with_id(2 + offset))
            .respond_with(mock_response().with_id(2 + offset));

        let response = setup
            .client(mocks)
            .with_rpc_sources(source.clone())
            .build()
            .get_block_by_number(BlockNumberOrTag::Latest)
            .send()
            .await
            .expect_consistent()
            .unwrap();

        assert_eq!(response, alloy_rpc_types::Block {
            header: alloy_rpc_types::Header {
                hash: b256!("0xc3674be7b9d95580d7f23c03d32e946f2b453679ee6505e3a778f003c5a3cfae"),
                inner: alloy_consensus::Header {
                    parent_hash: b256!("0x43325027f6adf9befb223f8ae80db057daddcd7b48e41f60cd94bfa8877181ae"),
                    ommers_hash: b256!("0x1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347"),
                    beneficiary: address!("0x388c818ca8b9251b393131c08a736a67ccb19297"),
                    state_root: b256!("0x13552447dd62f11ad885f21a583c4fa34144efe923c7e35fb018d6710f06b2b6"),
                    transactions_root: b256!("0x93a1ad3d067009259b508cc95fde63b5efd7e9d8b55754314c173fdde8c0826a"),
                    receipts_root: b256!("0x66934c3fd9c547036fe0e56ad01bc43c84b170be7c4030a86805ddcdab149929"),
                    logs_bloom: bloom!("0x3e6b8420e1a13038902c24d6c2a9720a7ad4860cdc870cd5c0490011e43631134f608935bd83171247407da2c15d85014f9984608c03684c74aad48b20bc24022134cdca5f2e9d2dee3b502a8ccd39eff8040b1d96601c460e119c408c620b44fa14053013220847045556ea70484e67ec012c322830cf56ef75e09bd0db28a00f238adfa587c9f80d7e30d3aba2863e63a5cad78954555966b1055a4936643366a0bb0b1bac68d0e6267fc5bf8304d404b0c69041125219aa70562e6a5a6362331a414a96d0716990a10161b87dd9568046a742d4280014975e232b6001a0360970e569d54404b27807d7a44c949ac507879d9d41ec8842122da6772101bc8b"),
                    difficulty: alloy_primitives::U256::ZERO,
                    number: 18_722_845_u64,
                    gas_limit: 0x1c9c380_u64,
                    gas_used: 0xa768c4_u64,
                    timestamp: 0x656f96f3_u64,
                    extra_data: bytes!("0x546974616e2028746974616e6275696c6465722e78797a29"),
                    mix_hash: b256!("0x516a58424d4883a3614da00a9c6f18cd5cd54335a08388229a993a8ecf05042f"),
                    nonce: alloy_primitives::B64::ZERO,
                    base_fee_per_gas: Some(57_750_497_844_u64),
                    withdrawals_root: None,
                    blob_gas_used: None,
                    excess_blob_gas: None,
                    parent_beacon_block_root: None,
                    requests_hash: None,
                },
                total_difficulty: None,
                size: Some(alloy_primitives::U256::from(0xcd35_u64)),
            },
            uncles: vec![],
            transactions: BlockTransactions::Hashes(vec![]),
            withdrawals: None,
        });
    }
}

#[tokio::test]
async fn eth_get_block_by_number_pre_london_fork_should_succeed() {
    fn mock_request() -> JsonRpcRequestMatcher {
        JsonRpcRequestMatcher::with_method("eth_getBlockByNumber")
            .with_params(json!(["latest", false]))
    }

    fn mock_response() -> JsonRpcResponse {
        JsonRpcResponse::from(json!({
           "jsonrpc":"2.0",
           "id":0,
           "result":{
              "number":"0x0",
              "hash":"0xd4e56740f876aef8c010b86a40d5f56745a118d0906a34e69aec8c0db1cb8fa3",
              "transactions":[],
              "totalDifficulty":"0x400000000",
              "logsBloom":"0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
              "receiptsRoot":"0x56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421",
              "extraData":"0x11bbe8db4e347b4e8c937c1c8370e4b5ed33adb3db69cbdb7a38e1e50b1b82fa",
              "nonce":"0x0000000000000042",
              "miner":"0x0000000000000000000000000000000000000000",
              "difficulty":"0x400000000",
              "gasLimit":"0x1388",
              "gasUsed":"0x0",
              "uncles":[],
              "sha3Uncles":"0x1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347",
              "size":"0x21c",
              "transactionsRoot":"0x56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421",
              "stateRoot":"0xd7f8974fb5ac78d9ac099b9ad5018bedc2ce0a72dad1827a1709da30580f0544",
              "mixHash":"0x0000000000000000000000000000000000000000000000000000000000000000",
              "parentHash":"0x0000000000000000000000000000000000000000000000000000000000000000",
              "timestamp":"0x0"
           }
        }))
    }

    let setup = EvmRpcNonblockingSetup::new().await.mock_api_keys().await;

    for (source, offset) in iter::zip(RPC_SERVICES, (0_u64..).step_by(3)) {
        let mocks = MockHttpOutcallsBuilder::new()
            .given(mock_request().with_id(offset))
            .respond_with(mock_response().with_id(offset))
            .given(mock_request().with_id(1 + offset))
            .respond_with(mock_response().with_id(1 + offset))
            .given(mock_request().with_id(2 + offset))
            .respond_with(mock_response().with_id(2 + offset));

        let response = setup
            .client(mocks)
            .with_rpc_sources(source.clone())
            .build()
            .get_block_by_number(BlockNumberOrTag::Latest)
            .send()
            .await
            .expect_consistent()
            .unwrap();

        assert_eq!(response, alloy_rpc_types::Block {
            header: alloy_rpc_types::Header {
                hash: b256!("0xd4e56740f876aef8c010b86a40d5f56745a118d0906a34e69aec8c0db1cb8fa3"),
                inner: alloy_consensus::Header {
                    parent_hash: b256!("0x0000000000000000000000000000000000000000000000000000000000000000"),
                    ommers_hash: b256!("0x1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347"),
                    beneficiary: address!("0x0000000000000000000000000000000000000000"),
                    state_root: b256!("0xd7f8974fb5ac78d9ac099b9ad5018bedc2ce0a72dad1827a1709da30580f0544"),
                    transactions_root: b256!("0x56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421"),
                    receipts_root: b256!("0x56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421"),
                    logs_bloom: bloom!("0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000"),
                    difficulty: alloy_primitives::U256::from(0x400000000_u64),
                    number: 0_u64,
                    gas_limit: 0x1388_u64,
                    gas_used: 0_u64,
                    timestamp: 0_u64,
                    extra_data: bytes!("0x11bbe8db4e347b4e8c937c1c8370e4b5ed33adb3db69cbdb7a38e1e50b1b82fa"),
                    mix_hash: b256!("0x0000000000000000000000000000000000000000000000000000000000000000"),
                    nonce: alloy_primitives::B64::from(0x0000000000000042_u64),
                    base_fee_per_gas: None,
                    withdrawals_root: None,
                    blob_gas_used: None,
                    excess_blob_gas: None,
                    parent_beacon_block_root: None,
                    requests_hash: None,
                },
                total_difficulty: None,
                size: Some(alloy_primitives::U256::from(0x21c_u64)),
            },
            uncles: vec![],
            transactions: BlockTransactions::Hashes(vec![]),
            withdrawals: None,
        });
    }
}

#[tokio::test]
async fn eth_get_block_by_number_should_be_consistent_when_total_difficulty_inconsistent() {
    fn mock_request() -> JsonRpcRequestMatcher {
        JsonRpcRequestMatcher::with_method("eth_getBlockByNumber")
            .with_params(json!(["latest", false]))
    }

    fn mock_response(total_difficulty: Option<&str>) -> JsonRpcResponse {
        let mut body = json!({
           "jsonrpc":"2.0",
           "result":{
              "baseFeePerGas":"0xd7232aa34",
              "difficulty":"0x0",
              "extraData":"0x546974616e2028746974616e6275696c6465722e78797a29",
              "gasLimit":"0x1c9c380",
              "gasUsed":"0xa768c4",
              "hash":"0xc3674be7b9d95580d7f23c03d32e946f2b453679ee6505e3a778f003c5a3cfae",
              "logsBloom":"0x3e6b8420e1a13038902c24d6c2a9720a7ad4860cdc870cd5c0490011e43631134f608935bd83171247407da2c15d85014f9984608c03684c74aad48b20bc24022134cdca5f2e9d2dee3b502a8ccd39eff8040b1d96601c460e119c408c620b44fa14053013220847045556ea70484e67ec012c322830cf56ef75e09bd0db28a00f238adfa587c9f80d7e30d3aba2863e63a5cad78954555966b1055a4936643366a0bb0b1bac68d0e6267fc5bf8304d404b0c69041125219aa70562e6a5a6362331a414a96d0716990a10161b87dd9568046a742d4280014975e232b6001a0360970e569d54404b27807d7a44c949ac507879d9d41ec8842122da6772101bc8b",
              "miner":"0x388c818ca8b9251b393131c08a736a67ccb19297",
              "mixHash":"0x516a58424d4883a3614da00a9c6f18cd5cd54335a08388229a993a8ecf05042f",
              "nonce":"0x0000000000000000",
              "number":"0x11db01d",
              "parentHash":"0x43325027f6adf9befb223f8ae80db057daddcd7b48e41f60cd94bfa8877181ae",
              "receiptsRoot":"0x66934c3fd9c547036fe0e56ad01bc43c84b170be7c4030a86805ddcdab149929",
              "sha3Uncles":"0x1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347",
              "size":"0xcd35",
              "stateRoot":"0x13552447dd62f11ad885f21a583c4fa34144efe923c7e35fb018d6710f06b2b6",
              "timestamp":"0x656f96f3",
              "withdrawalsRoot":"0xecae44b2c53871003c5cc75285995764034c9b5978a904229d36c1280b141d48",
              "transactionsRoot":"0x93a1ad3d067009259b508cc95fde63b5efd7e9d8b55754314c173fdde8c0826a",
           },
           "id":0
        });
        if let Some(total_difficulty) = total_difficulty {
            body.get_mut("result").unwrap()["totalDifficulty"] =
                Value::String(total_difficulty.to_string());
        }
        JsonRpcResponse::from(body)
    }

    let setup = EvmRpcNonblockingSetup::new().await.mock_api_keys().await;

    let mocks = MockHttpOutcallsBuilder::new()
        .given(mock_request().with_id(0_u64))
        .respond_with(mock_response(Some("0xc70d815d562d3cfa955")).with_id(0_u64))
        .given(mock_request().with_id(1_u64))
        .respond_with(mock_response(None).with_id(1_u64));

    let response = setup
        .client(mocks)
        .with_rpc_sources(RpcServices::EthMainnet(Some(vec![
            EthMainnetService::Ankr,
            EthMainnetService::PublicNode,
        ])))
        .build()
        .get_block_by_number(BlockNumberOrTag::Latest)
        .send()
        .await
        .expect_consistent()
        .unwrap();

    assert_eq!(response.number(), 18_722_845_u64);
    assert_eq!(response.header.total_difficulty, None);
}

#[test]
fn eth_get_transaction_receipt_should_succeed() {
    let test_cases = [
        TestCase {
            request: "0xdd5d4b18923d7aae953c7996d791118102e889bea37b48a651157a4890e4746f",
            raw_body: json!({"jsonrpc":"2.0","id":0,"result":{"blockHash":"0x5115c07eb1f20a9d6410db0916ed3df626cfdab161d3904f45c8c8b65c90d0be","blockNumber":"0x11a85ab","contractAddress":null,"cumulativeGasUsed":"0xf02aed","effectiveGasPrice":"0x63c00ee76","from":"0x0aa8ebb6ad5a8e499e550ae2c461197624c6e667","gasUsed":"0x7d89","logs":[],"logsBloom":"0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000","status":"0x1","to":"0x356cfd6e6d0000400000003900b415f80669009e","transactionHash":"0xdd5d4b18923d7aae953c7996d791118102e889bea37b48a651157a4890e4746f","transactionIndex":"0xd9","type":"0x2"}}),
            expected: evm_rpc_types::TransactionReceipt {
                status: Some(0x1_u8.into()),
                root: None,
                transaction_hash: "0xdd5d4b18923d7aae953c7996d791118102e889bea37b48a651157a4890e4746f".parse().unwrap(),
                contract_address: None,
                block_number: 0x11a85ab_u64.into(),
                block_hash: "0x5115c07eb1f20a9d6410db0916ed3df626cfdab161d3904f45c8c8b65c90d0be".parse().unwrap(),
                effective_gas_price: 0x63c00ee76_u64.into(),
                gas_used: 0x7d89_u32.into(),
                from: "0x0aa8ebb6ad5a8e499e550ae2c461197624c6e667".parse().unwrap(),
                logs: vec![],
                logs_bloom: "0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000".parse().unwrap(),
                to: Some("0x356cfd6e6d0000400000003900b415f80669009e".parse().unwrap()),
                transaction_index: 0xd9_u16.into(),
                tx_type: "0x2".parse().unwrap(),
                cumulative_gas_used: 0xf02aed_u64.into(),
            },
        },
        TestCase { //first transaction after genesis
            request: "0x5c504ed432cb51138bcf09aa5e8a410dd4a1e204ef84bfed1be16dfba1b22060",
            raw_body: json!({"jsonrpc":"2.0","id":0,"result":{"transactionHash":"0x5c504ed432cb51138bcf09aa5e8a410dd4a1e204ef84bfed1be16dfba1b22060","blockHash":"0x4e3a3754410177e6937ef1f84bba68ea139e8d1a2258c5f85db9f1cd715a1bdd","blockNumber":"0xb443","logsBloom":"0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000","gasUsed":"0x5208","root":"0x96a8e009d2b88b1483e6941e6812e32263b05683fac202abc622a3e31aed1957","contractAddress":null,"cumulativeGasUsed":"0x5208","transactionIndex":"0x0","from":"0xa1e4380a3b1f749673e270229993ee55f35663b4","to":"0x5df9b87991262f6ba471f09758cde1c0fc1de734","type":"0x0","effectiveGasPrice":"0x2d79883d2000","logs":[],"root":"0x96a8e009d2b88b1483e6941e6812e32263b05683fac202abc622a3e31aed1957"}}),
            expected: evm_rpc_types::TransactionReceipt {
                status: None,
                root: Some("0x96a8e009d2b88b1483e6941e6812e32263b05683fac202abc622a3e31aed1957".parse().unwrap()),
                transaction_hash: "0x5c504ed432cb51138bcf09aa5e8a410dd4a1e204ef84bfed1be16dfba1b22060".parse().unwrap(),
                contract_address: None,
                block_number: 0xb443_u64.into(),
                block_hash: "0x4e3a3754410177e6937ef1f84bba68ea139e8d1a2258c5f85db9f1cd715a1bdd".parse().unwrap(),
                effective_gas_price: 0x2d79883d2000_u64.into(),
                gas_used: 0x5208_u32.into(),
                from: "0xa1e4380a3b1f749673e270229993ee55f35663b4".parse().unwrap(),
                logs: vec![],
                logs_bloom: "0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000".parse().unwrap(),
                to: Some("0x5df9b87991262f6ba471f09758cde1c0fc1de734".parse().unwrap()),
                transaction_index: 0x0_u16.into(),
                tx_type: "0x0".parse().unwrap(),
                cumulative_gas_used: 0x5208_u64.into(),
            },
        },
        TestCase { //contract creation
            request: "0x2b8e12d42a187ace19c64b47fae0955def8859bf966c345102c6d3a52f28308b",
            raw_body: json!({"jsonrpc":"2.0","id":0,"result":{"transactionHash":"0x2b8e12d42a187ace19c64b47fae0955def8859bf966c345102c6d3a52f28308b","blockHash":"0xd050426a753a7cc4833ba15a5dfcef761fd983f5277230ea8dc700eadd307363","blockNumber":"0x12e64fd","logsBloom":"0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000","gasUsed":"0x69892","contractAddress":"0x6abda0438307733fc299e9c229fd3cc074bd8cc0","cumulativeGasUsed":"0x3009d2","transactionIndex":"0x17","from":"0xe12e9a6661aeaf57abf95fd060bebb223fbee7dd","to":null,"type":"0x2","effectiveGasPrice":"0x17c01a135","logs":[],"status":"0x1"}}),
            expected: evm_rpc_types::TransactionReceipt {
                status: Some(0x1_u8.into()),
                root: None,
                transaction_hash: "0x2b8e12d42a187ace19c64b47fae0955def8859bf966c345102c6d3a52f28308b".parse().unwrap(),
                contract_address: Some("0x6abda0438307733fc299e9c229fd3cc074bd8cc0".parse().unwrap()),
                block_number: 0x12e64fd_u64.into(),
                block_hash: "0xd050426a753a7cc4833ba15a5dfcef761fd983f5277230ea8dc700eadd307363".parse().unwrap(),
                effective_gas_price: 0x17c01a135_u64.into(),
                gas_used: 0x69892_u32.into(),
                from: "0xe12e9a6661aeaf57abf95fd060bebb223fbee7dd".parse().unwrap(),
                logs: vec![],
                logs_bloom: "0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000".parse().unwrap(),
                to: None,
                transaction_index: 0x17_u16.into(),
                tx_type: "0x2".parse().unwrap(),
                cumulative_gas_used: 0x3009d2_u64.into(),
            },
        }
    ];

    let mut offset = 0_u64;
    let setup = EvmRpcSetup::new().mock_api_keys();
    for test_case in test_cases {
        for source in RPC_SERVICES {
            let mut responses: [serde_json::Value; 3] =
                json_rpc_sequential_id(test_case.raw_body.clone());
            add_offset_json_rpc_id(responses.as_mut_slice(), offset);
            let response = setup
                .eth_get_transaction_receipt(source.clone(), None, test_case.request)
                .mock_http_once(MockOutcallBuilder::new(200, responses[0].clone()))
                .mock_http_once(MockOutcallBuilder::new(200, responses[1].clone()))
                .mock_http_once(MockOutcallBuilder::new(200, responses[2].clone()))
                .wait()
                .expect_consistent()
                .unwrap();

            assert_eq!(response, Some(test_case.expected.clone()));
            offset += 3;
        }
    }
}

#[tokio::test]
async fn eth_get_transaction_count_should_succeed() {
    let setup = EvmRpcNonblockingSetup::new().await.mock_api_keys().await;

    for (source, offset) in iter::zip(RPC_SERVICES, (0_u64..).step_by(3)) {
        let mocks = MockHttpOutcallsBuilder::new()
            .given(get_transaction_count_request().with_id(offset))
            .respond_with(get_transaction_count_response().with_id(offset))
            .given(get_transaction_count_request().with_id(offset + 1))
            .respond_with(get_transaction_count_response().with_id(offset + 1))
            .given(get_transaction_count_request().with_id(offset + 2))
            .respond_with(get_transaction_count_response().with_id(offset + 2));

        let response = setup
            .client(mocks)
            .with_rpc_sources(source.clone())
            .build()
            .get_transaction_count((
                address!("0xdac17f958d2ee523a2206206994597c13d831ec7"),
                BlockNumberOrTag::Latest,
            ))
            .send()
            .await
            .expect_consistent();

        assert_eq!(response, Ok(U256::ONE));
    }
}

#[tokio::test]
async fn eth_fee_history_should_succeed() {
    fn mock_request() -> JsonRpcRequestMatcher {
        JsonRpcRequestMatcher::with_method("eth_feeHistory").with_params(json!([
            "0x3",
            "latest",
            []
        ]))
    }

    fn mock_response() -> JsonRpcResponse {
        JsonRpcResponse::from(json!({
            "id" : 0,
            "jsonrpc" : "2.0",
            "result" : {
                "oldestBlock" : "0x11e57f5",
                "baseFeePerGas" : ["0x9cf6c61b9", "0x97d853982", "0x9ba55a0b0", "0x9543bf98d"],
                "reward" : [["0x0123"]]
            }
        }))
    }

    let setup = EvmRpcNonblockingSetup::new().await.mock_api_keys().await;

    for (source, offset) in iter::zip(RPC_SERVICES, (0_u64..).step_by(3)) {
        let mocks = MockHttpOutcallsBuilder::new()
            .given(mock_request().with_id(offset))
            .respond_with(mock_response().with_id(offset))
            .given(mock_request().with_id(1 + offset))
            .respond_with(mock_response().with_id(1 + offset))
            .given(mock_request().with_id(2 + offset))
            .respond_with(mock_response().with_id(2 + offset));

        let response = setup
            .client(mocks)
            .with_rpc_sources(source.clone())
            .build()
            .fee_history((3_u64, BlockNumberOrTag::Latest))
            .send()
            .await
            .expect_consistent()
            .unwrap();

        assert_eq!(
            response,
            alloy_rpc_types::FeeHistory {
                oldest_block: 0x11e57f5_u64,
                base_fee_per_gas: vec![0x9cf6c61b9_u128, 0x97d853982, 0x9ba55a0b0, 0x9543bf98d],
                gas_used_ratio: vec![],
                reward: Some(vec![vec![0x0123_u128]]),
                base_fee_per_blob_gas: vec![],
                blob_gas_used_ratio: vec![],
            }
        );
    }
}

#[test]
fn eth_send_raw_transaction_should_succeed() {
    let [response_0, response_1, response_2] =
        json_rpc_sequential_id(json!({"id":0,"jsonrpc":"2.0","result":"Ok"}));
    for source in RPC_SERVICES {
        let setup = EvmRpcSetup::new().mock_api_keys();
        let response = setup
            .eth_send_raw_transaction(source.clone(), None, MOCK_TRANSACTION)
            .mock_http_once(MockOutcallBuilder::new(200, response_0.clone()))
            .mock_http_once(MockOutcallBuilder::new(200, response_1.clone()))
            .mock_http_once(MockOutcallBuilder::new(200, response_2.clone()))
            .wait()
            .expect_consistent()
            .unwrap();
        assert_eq!(
            response,
            evm_rpc_types::SendRawTransactionStatus::Ok(Some(
                Hex32::from_str(MOCK_TRANSACTION_HASH).unwrap()
            ))
        );
    }
}

#[tokio::test]
async fn eth_call_should_succeed() {
    const ADDRESS: &str = "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48";
    const INPUT_DATA: &str =
        "0x70a08231000000000000000000000000b25eA1D493B49a1DeD42aC5B1208cC618f9A9B80";

    fn mock_request() -> JsonRpcRequestMatcher {
        JsonRpcRequestMatcher::with_method("eth_call").with_params(json!( [ { "to": ADDRESS.to_lowercase(), "input": INPUT_DATA.to_lowercase(), }, "latest" ]))
    }

    fn mock_response() -> JsonRpcResponse {
        JsonRpcResponse::from(
            json!({ "jsonrpc": "2.0", "id": 0, "result": "0x0000000000000000000000000000000000000000000000000000013c3ee36e89" }),
        )
    }

    let setup = EvmRpcNonblockingSetup::new().await.mock_api_keys().await;

    let mut offsets = (0_u64..).step_by(3);
    for call_args in [
        evm_rpc_types::CallArgs {
            transaction: evm_rpc_types::TransactionRequest {
                to: Some(ADDRESS.parse().unwrap()),
                input: Some(INPUT_DATA.parse().unwrap()),
                ..evm_rpc_types::TransactionRequest::default()
            },
            block: Some(BlockTag::Latest),
        },
        evm_rpc_types::CallArgs {
            transaction: evm_rpc_types::TransactionRequest {
                to: Some(ADDRESS.parse().unwrap()),
                input: Some(INPUT_DATA.parse().unwrap()),
                ..evm_rpc_types::TransactionRequest::default()
            },
            block: None, //should be same as specifying Latest
        },
    ] {
        for source in RPC_SERVICES {
            let offset = offsets.next().unwrap();
            let mocks = MockHttpOutcallsBuilder::new()
                .given(mock_request().with_id(offset))
                .respond_with(mock_response().with_id(offset))
                .given(mock_request().with_id(offset + 1))
                .respond_with(mock_response().with_id(offset + 1))
                .given(mock_request().with_id(offset + 2))
                .respond_with(mock_response().with_id(offset + 2));

            let response = setup
                .client(mocks)
                .with_rpc_sources(source.clone())
                .build()
                .call(call_args.clone())
                .send()
                .await
                .expect_consistent()
                .unwrap();
            assert_eq!(
                response,
                bytes!("0x0000000000000000000000000000000000000000000000000000013c3ee36e89")
            );
        }
    }
}

#[test]
fn candid_rpc_should_allow_unexpected_response_fields() {
    let setup = EvmRpcSetup::new().mock_api_keys();
    let [response_0, response_1, response_2] = json_rpc_sequential_id(
        json!({"jsonrpc":"2.0","id":0,"result":{"unexpectedKey":"unexpectedValue","blockHash":"0xb3b20624f8f0f86eb50dd04688409e5cea4bd02d700bf6e79e9384d47d6a5a35","blockNumber":"0x5bad55","contractAddress":null,"cumulativeGasUsed":"0xb90b0","effectiveGasPrice":"0x746a528800","from":"0x398137383b3d25c92898c656696e41950e47316b","gasUsed":"0x1383f","logs":[],"logsBloom":"0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000","status":"0x1","to":"0x06012c8cf97bead5deae237070f9587f8e7a266d","transactionHash":"0xbb3a336e3f823ec18197f1e13ee875700f08f03e2cab75f0d0b118dabb44cba0","transactionIndex":"0x11","type":"0x0"}}),
    );
    let response = setup
        .eth_get_transaction_receipt(
            RpcServices::EthMainnet(None),
            None,
            "0xdd5d4b18923d7aae953c7996d791118102e889bea37b48a651157a4890e4746f",
        )
        .mock_http_once(MockOutcallBuilder::new(200, response_0.clone()))
        .mock_http_once(MockOutcallBuilder::new(200, response_1.clone()))
        .mock_http_once(MockOutcallBuilder::new(200, response_2.clone()))
        .wait()
        .expect_consistent()
        .unwrap()
        .expect("receipt was None");
    assert_eq!(
        response.block_hash,
        "0xb3b20624f8f0f86eb50dd04688409e5cea4bd02d700bf6e79e9384d47d6a5a35"
            .parse()
            .unwrap()
    );
}

#[test]
fn candid_rpc_should_err_without_cycles() {
    let setup = EvmRpcSetup::with_args(InstallArgs {
        demo: None,
        ..Default::default()
    })
    .mock_api_keys();
    let result = setup
        .eth_get_transaction_receipt(
            RpcServices::EthMainnet(None),
            None,
            "0xdd5d4b18923d7aae953c7996d791118102e889bea37b48a651157a4890e4746f",
        )
        .wait()
        .expect_inconsistent();
    // Because the expected cycles are different for each provider, the results are inconsistent
    // but should all be `TooFewCycles` error.
    for (_, err) in result {
        assert_matches!(
            err,
            Err(RpcError::ProviderError(ProviderError::TooFewCycles {
                expected: _,
                received: 0,
            }))
        )
    }
}

#[test]
fn candid_rpc_should_err_with_insufficient_cycles() {
    let setup = EvmRpcSetup::with_args(InstallArgs {
        demo: Some(true),
        nodes_in_subnet: Some(33),
        ..Default::default()
    })
    .mock_api_keys();
    let mut result = setup
        .eth_get_transaction_receipt(
            RpcServices::EthMainnet(None),
            None,
            "0xdd5d4b18923d7aae953c7996d791118102e889bea37b48a651157a4890e4746f",
        )
        .wait()
        .expect_inconsistent();
    let regex = regex::Regex::new(
        "http_request request sent with [0-9_]+ cycles, but [0-9_]+ cycles are required.",
    )
    .unwrap();
    assert_matches!(
        result.pop().unwrap(),
        (
            RpcService::EthMainnet(EthMainnetService::PublicNode),
            Err(RpcError::HttpOutcallError(HttpOutcallError::IcError {
                code: LegacyRejectionCode::CanisterReject,
                message
            }))
        ) if regex.is_match(&message)
    );

    // Same request should succeed after upgrade to the expected node count
    setup.upgrade_canister(InstallArgs {
        nodes_in_subnet: Some(34),
        ..Default::default()
    });
    let [response_0, response_1, response_2] = json_rpc_sequential_id(
        json!({"jsonrpc":"2.0","id":0,"result":{"blockHash":"0x5115c07eb1f20a9d6410db0916ed3df626cfdab161d3904f45c8c8b65c90d0be","blockNumber":"0x11a85ab","contractAddress":null,"cumulativeGasUsed":"0xf02aed","effectiveGasPrice":"0x63c00ee76","from":"0x0aa8ebb6ad5a8e499e550ae2c461197624c6e667","gasUsed":"0x7d89","logs":[],"logsBloom":"0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000","status":"0x1","to":"0x356cfd6e6d0000400000003900b415f80669009e","transactionHash":"0xdd5d4b18923d7aae953c7996d791118102e889bea37b48a651157a4890e4746f","transactionIndex":"0xd9","type":"0x2"}}),
    );

    let result = setup
        .eth_get_transaction_receipt(
            RpcServices::EthMainnet(None),
            None,
            "0xdd5d4b18923d7aae953c7996d791118102e889bea37b48a651157a4890e4746f",
        )
        .mock_http_once(MockOutcallBuilder::new(200, response_0))
        .mock_http_once(MockOutcallBuilder::new(200, response_1))
        .mock_http_once(MockOutcallBuilder::new(200, response_2))
        .wait()
        .expect_consistent()
        .unwrap();
    assert_matches!(result, Some(evm_rpc_types::TransactionReceipt { .. }));
}

#[test]
fn candid_rpc_should_err_when_service_unavailable() {
    let setup = EvmRpcSetup::new().mock_api_keys();
    let result = setup
        .eth_get_transaction_receipt(
            RpcServices::EthMainnet(None),
            None,
            "0xdd5d4b18923d7aae953c7996d791118102e889bea37b48a651157a4890e4746f",
        )
        .mock_http(MockOutcallBuilder::new(503, "Service unavailable"))
        .wait()
        .expect_consistent();
    assert_eq!(
        result,
        Err(RpcError::HttpOutcallError(
            HttpOutcallError::InvalidHttpJsonRpcResponse {
                status: 503,
                body: "Service unavailable".to_string(),
                parsing_error: None
            }
        ))
    );
    let rpc_method = || RpcMethod::EthGetTransactionReceipt.into();
    assert_eq!(
        setup.get_metrics(),
        Metrics {
            requests: hashmap! {
                (rpc_method(), BLOCKPI_ETH_HOSTNAME.into()) => 1,
                (rpc_method(), ANKR_HOSTNAME.into()) => 1,
                (rpc_method(), PUBLICNODE_ETH_MAINNET_HOSTNAME.into()) => 1,
            },
            responses: hashmap! {
                (rpc_method(), BLOCKPI_ETH_HOSTNAME.into(), 503.into()) => 1,
                (rpc_method(), ANKR_HOSTNAME.into(), 503.into()) => 1,
                (rpc_method(), PUBLICNODE_ETH_MAINNET_HOSTNAME.into(), 503.into()) => 1,
            },
            ..Default::default()
        }
    );
}

#[test]
fn candid_rpc_should_recognize_json_error() {
    let setup = EvmRpcSetup::new().mock_api_keys();
    let [response_0, response_1] = json_rpc_sequential_id(
        json!({"jsonrpc":"2.0","id":0,"error":{"code":123,"message":"Error message"}}),
    );
    let result = setup
        .eth_get_transaction_receipt(
            RpcServices::EthSepolia(Some(vec![
                EthSepoliaService::Ankr,
                EthSepoliaService::BlockPi,
            ])),
            None,
            "0xdd5d4b18923d7aae953c7996d791118102e889bea37b48a651157a4890e4746f",
        )
        .mock_http_once(MockOutcallBuilder::new(200, response_0))
        .mock_http_once(MockOutcallBuilder::new(200, response_1))
        .wait()
        .expect_consistent();
    assert_eq!(
        result,
        Err(RpcError::JsonRpcError(JsonRpcError {
            code: 123,
            message: "Error message".to_string(),
        }))
    );
    let rpc_method = || RpcMethod::EthGetTransactionReceipt.into();
    assert_eq!(
        setup.get_metrics(),
        Metrics {
            requests: hashmap! {
                (rpc_method(), ANKR_HOSTNAME.into()) => 1,
                (rpc_method(), BLOCKPI_ETH_SEPOLIA_HOSTNAME.into()) => 1,
            },
            responses: hashmap! {
                (rpc_method(), ANKR_HOSTNAME.into(), 200.into()) => 1,
                (rpc_method(), BLOCKPI_ETH_SEPOLIA_HOSTNAME.into(), 200.into()) => 1,
            },
            ..Default::default()
        }
    );
}

#[test]
fn candid_rpc_should_reject_empty_service_list() {
    let setup = EvmRpcSetup::new().mock_api_keys();
    let result = setup
        .eth_get_transaction_receipt(
            RpcServices::EthMainnet(Some(vec![])),
            None,
            "0xdd5d4b18923d7aae953c7996d791118102e889bea37b48a651157a4890e4746f",
        )
        .wait()
        .expect_consistent();
    assert_eq!(
        result,
        Err(RpcError::ProviderError(ProviderError::ProviderNotFound))
    );
}

#[test]
fn candid_rpc_should_return_inconsistent_results() {
    let setup = EvmRpcSetup::new().mock_api_keys();
    let results = setup
        .eth_send_raw_transaction(
            RpcServices::EthMainnet(Some(vec![
                EthMainnetService::Ankr,
                EthMainnetService::Cloudflare,
            ])),
            None,
            MOCK_TRANSACTION,
        )
        .mock_http_once(MockOutcallBuilder::new(
            200,
            r#"{"id":0,"jsonrpc":"2.0","result":"Ok"}"#,
        ))
        .mock_http_once(MockOutcallBuilder::new(
            200,
            r#"{"id":1,"jsonrpc":"2.0","result":"NonceTooLow"}"#,
        ))
        .wait()
        .expect_inconsistent();
    assert_eq!(
        results,
        vec![
            (
                RpcService::EthMainnet(EthMainnetService::Ankr),
                Ok(evm_rpc_types::SendRawTransactionStatus::Ok(Some(
                    Hex32::from_str(MOCK_TRANSACTION_HASH).unwrap()
                )))
            ),
            (
                RpcService::EthMainnet(EthMainnetService::Cloudflare),
                Ok(evm_rpc_types::SendRawTransactionStatus::NonceTooLow)
            )
        ]
    );
    let rpc_method = || RpcMethod::EthSendRawTransaction.into();
    assert_eq!(
        setup.get_metrics(),
        Metrics {
            requests: hashmap! {
                (rpc_method(), ANKR_HOSTNAME.into()) => 1,
                (rpc_method(), CLOUDFLARE_HOSTNAME.into()) => 1,
            },
            responses: hashmap! {
                (rpc_method(), ANKR_HOSTNAME.into(), 200.into()) => 1,
                (rpc_method(), CLOUDFLARE_HOSTNAME.into(), 200.into()) => 1,
            },
            inconsistent_responses: hashmap! {
                (rpc_method(), ANKR_HOSTNAME.into()) => 1,
                (rpc_method(), CLOUDFLARE_HOSTNAME.into()) => 1,
            },
            ..Default::default()
        }
    );
}

#[tokio::test]
async fn candid_rpc_should_return_3_out_of_4_transaction_count() {
    fn get_transaction_count_response(result: u64) -> JsonRpcResponse {
        JsonRpcResponse::from(
            json!({ "jsonrpc" : "2.0", "id" : 0, "result" : format!("0x{result:x}") }),
        )
    }

    let setup = EvmRpcNonblockingSetup::new().await.mock_api_keys().await;

    async fn eth_get_transaction_count_with_3_out_of_4(
        setup: &EvmRpcNonblockingSetup,
        offset: u64,
        [response0, response1, response2, response3]: [CanisterHttpResponse; 4],
    ) -> MultiRpcResult<U256> {
        let mocks = MockHttpOutcallsBuilder::new()
            .given(get_transaction_count_request().with_id(offset))
            .respond_with(response0)
            .given(get_transaction_count_request().with_id(offset + 1))
            .respond_with(response1)
            .given(get_transaction_count_request().with_id(offset + 2))
            .respond_with(response2)
            .given(get_transaction_count_request().with_id(offset + 3))
            .respond_with(response3);

        setup
            .client(mocks)
            .with_rpc_sources(RpcServices::EthMainnet(None))
            .with_consensus_strategy(ConsensusStrategy::Threshold {
                total: Some(4),
                min: 3,
            })
            .build()
            .get_transaction_count((
                address!("0xdac17f958d2ee523a2206206994597c13d831ec7"),
                BlockNumberOrTag::Latest,
            ))
            .send()
            .await
    }

    for (successful_mocks, offset) in [
        [
            get_transaction_count_response(1).with_id(0_u64).into(),
            get_transaction_count_response(1).with_id(1_u64).into(),
            get_transaction_count_response(1).with_id(2_u64).into(),
            get_transaction_count_response(1).with_id(3_u64).into(),
        ],
        [
            get_transaction_count_response(1).with_id(4_u64).into(),
            CanisterHttpReply::with_status(500)
                .with_body("OFFLINE")
                .into(),
            get_transaction_count_response(1).with_id(6_u64).into(),
            get_transaction_count_response(1).with_id(7_u64).into(),
        ],
        [
            get_transaction_count_response(1).with_id(8_u64).into(),
            get_transaction_count_response(1).with_id(9_u64).into(),
            get_transaction_count_response(2).with_id(10_u64).into(),
            get_transaction_count_response(1).with_id(11_u64).into(),
        ],
    ]
    .into_iter()
    .zip((0_u64..).step_by(4))
    {
        let result = eth_get_transaction_count_with_3_out_of_4(&setup, offset, successful_mocks)
            .await
            .expect_consistent()
            .unwrap();

        assert_eq!(result, U256::ONE);
    }

    for (error_mocks, offset) in [
        [
            get_transaction_count_response(1).with_id(12_u64).into(),
            CanisterHttpReply::with_status(500)
                .with_body("OFFLINE")
                .into(),
            get_transaction_count_response(2).into(),
            get_transaction_count_response(1).with_id(15_u64).into(),
        ],
        [
            CanisterHttpReply::with_status(500)
                .with_body("FORBIDDEN")
                .into(),
            CanisterHttpReply::with_status(500)
                .with_body("OFFLINE")
                .into(),
            get_transaction_count_response(1).with_id(18_u64).into(),
            get_transaction_count_response(1).with_id(19_u64).into(),
        ],
        [
            get_transaction_count_response(1).with_id(20_u64).into(),
            get_transaction_count_response(3).with_id(21_u64).into(),
            get_transaction_count_response(2).with_id(22_u64).into(),
            get_transaction_count_response(1).with_id(23_u64).into(),
        ],
    ]
    .into_iter()
    .zip((12_u64..).step_by(4))
    {
        let result = eth_get_transaction_count_with_3_out_of_4(&setup, offset, error_mocks)
            .await
            .expect_inconsistent();

        assert_eq!(result.len(), 4);
    }
}

#[tokio::test]
async fn candid_rpc_should_return_inconsistent_results_with_error() {
    let setup = EvmRpcNonblockingSetup::new().await.mock_api_keys().await;

    let mocks = MockHttpOutcallsBuilder::new()
        .given(get_transaction_count_request().with_id(0_u64))
        .respond_with(get_transaction_count_response().with_id(0_u64))
        .given(get_transaction_count_request().with_id(1_u64))
        .respond_with(JsonRpcResponse::from(
            json!({"jsonrpc": "2.0", "id": 1, "error" : { "code": 123, "message": "Unexpected"} }),
        ));

    let result = setup
        .client(mocks)
        .with_rpc_sources(RpcServices::EthMainnet(Some(vec![
            EthMainnetService::Alchemy,
            EthMainnetService::Ankr,
        ])))
        .build()
        .get_transaction_count((
            address!("0xdac17f958d2ee523a2206206994597c13d831ec7"),
            BlockNumberOrTag::Latest,
        ))
        .send()
        .await
        .expect_inconsistent();

    assert_eq!(
        result,
        vec![
            (
                RpcService::EthMainnet(EthMainnetService::Alchemy),
                Ok(U256::ONE)
            ),
            (
                RpcService::EthMainnet(EthMainnetService::Ankr),
                Err(RpcError::JsonRpcError(JsonRpcError {
                    code: 123,
                    message: "Unexpected".to_string(),
                }))
            ),
        ]
    );
    let rpc_method = || RpcMethod::EthGetTransactionCount.into();
    assert_eq!(
        setup.get_metrics().await,
        Metrics {
            requests: hashmap! {
                (rpc_method(), ALCHEMY_ETH_MAINNET_HOSTNAME.into()) => 1,
                (rpc_method(), ANKR_HOSTNAME.into()) => 1,
            },
            responses: hashmap! {
                (rpc_method(), ALCHEMY_ETH_MAINNET_HOSTNAME.into(), 200.into()) => 1,
                (rpc_method(), ANKR_HOSTNAME.into(), 200.into()) => 1,
            },
            inconsistent_responses: hashmap! {
                (rpc_method(), ALCHEMY_ETH_MAINNET_HOSTNAME.into()) => 1,
                (rpc_method(), ANKR_HOSTNAME.into()) => 1,
            },
            ..Default::default()
        }
    );
}

#[tokio::test]
async fn candid_rpc_should_return_inconsistent_results_with_consensus_error() {
    const CONSENSUS_ERROR: &str =
        "No consensus could be reached. Replicas had different responses.";

    let setup = EvmRpcNonblockingSetup::new().await.mock_api_keys().await;

    let mocks = MockHttpOutcallsBuilder::new()
        .given(get_transaction_count_request().with_id(0_u64))
        .respond_with(
            CanisterHttpReject::with_reject_code(RejectCode::SysTransient)
                .with_message(CONSENSUS_ERROR),
        )
        .given(get_transaction_count_request().with_id(1_u64))
        .respond_with(get_transaction_count_response().with_id(1_u64))
        .given(get_transaction_count_request().with_id(2_u64))
        .respond_with(
            CanisterHttpReject::with_reject_code(RejectCode::SysTransient)
                .with_message(CONSENSUS_ERROR),
        );

    let result = setup
        .client(mocks)
        .with_rpc_sources(RpcServices::EthMainnet(None))
        .with_consensus_strategy(ConsensusStrategy::Threshold {
            total: Some(3),
            min: 2,
        })
        .build()
        .get_transaction_count((
            address!("0xdac17f958d2ee523a2206206994597c13d831ec7"),
            BlockNumberOrTag::Latest,
        ))
        .send()
        .await
        .expect_inconsistent();

    assert_eq!(
        result,
        vec![
            (
                RpcService::EthMainnet(EthMainnetService::BlockPi),
                Ok(U256::ONE)
            ),
            (
                RpcService::EthMainnet(EthMainnetService::Ankr),
                Err(RpcError::HttpOutcallError(HttpOutcallError::IcError {
                    code: LegacyRejectionCode::SysTransient,
                    message: CONSENSUS_ERROR.to_string()
                }))
            ),
            (
                RpcService::EthMainnet(EthMainnetService::PublicNode),
                Err(RpcError::HttpOutcallError(HttpOutcallError::IcError {
                    code: LegacyRejectionCode::SysTransient,
                    message: CONSENSUS_ERROR.to_string()
                }))
            ),
        ]
    );

    let rpc_method = || RpcMethod::EthGetTransactionCount.into();
    let err_http_outcall = setup.get_metrics().await.err_http_outcall;
    assert_eq!(
        err_http_outcall,
        hashmap! {
            (rpc_method(), ANKR_HOSTNAME.into(), LegacyRejectionCode::SysTransient) => 1,
            (rpc_method(), PUBLICNODE_ETH_MAINNET_HOSTNAME.into(), LegacyRejectionCode::SysTransient) => 1,
        },
    );
}

#[test]
fn should_have_metrics_for_generic_request() {
    use evm_rpc::types::MetricRpcMethod;

    let setup = EvmRpcSetup::new().mock_api_keys();
    let response = setup
        .request(
            RpcService::Custom(RpcApi {
                url: MOCK_REQUEST_URL.to_string(),
                headers: None,
            }),
            MOCK_REQUEST_PAYLOAD,
            MOCK_REQUEST_RESPONSE_BYTES,
        )
        .mock_http(MockOutcallBuilder::new(200, MOCK_REQUEST_RESPONSE))
        .wait();
    assert_eq!(response, Ok(MOCK_REQUEST_RESPONSE.to_string()));

    let rpc_method = || MetricRpcMethod("request".to_string());
    assert_eq!(
        setup.get_metrics(),
        Metrics {
            requests: hashmap! {
                (rpc_method(), CLOUDFLARE_HOSTNAME.into()) => 1,
            },
            responses: hashmap! {
                (rpc_method(), CLOUDFLARE_HOSTNAME.into(), 200.into()) => 1,
            },
            ..Default::default()
        }
    );
}

#[tokio::test]
async fn candid_rpc_should_return_inconsistent_results_with_unexpected_http_status() {
    let setup = EvmRpcNonblockingSetup::new().await.mock_api_keys().await;

    let mocks = MockHttpOutcallsBuilder::new()
        .given(get_transaction_count_request().with_id(0_u64))
        .respond_with(get_transaction_count_response().with_id(0_u64))
        .given(get_transaction_count_request().with_id(1_u64))
        .respond_with(CanisterHttpReply::with_status(400).with_body(
            json!({"jsonrpc": "2.0", "id": 1, "error": {"code": 123, "message": "Error message"}}),
        ));

    let result = setup
        .client(mocks)
        .with_rpc_sources(RpcServices::EthMainnet(Some(vec![
            EthMainnetService::Alchemy,
            EthMainnetService::Ankr,
        ])))
        .build()
        .get_transaction_count((
            address!("0xdac17f958d2ee523a2206206994597c13d831ec7"),
            BlockNumberOrTag::Latest,
        ))
        .send()
        .await
        .expect_inconsistent();
    assert_eq!(
        result,
        vec![
            (
                RpcService::EthMainnet(EthMainnetService::Alchemy),
                Ok(U256::ONE)
            ),
            (
                RpcService::EthMainnet(EthMainnetService::Ankr),
                Err(RpcError::HttpOutcallError(HttpOutcallError::InvalidHttpJsonRpcResponse {
                    status: 400,
                    body: "{\"error\":{\"code\":123,\"message\":\"Error message\"},\"id\":1,\"jsonrpc\":\"2.0\"}".to_string(),
                    parsing_error: None,
                })),
            ),
        ]
    );
    let rpc_method = || RpcMethod::EthGetTransactionCount.into();
    assert_eq!(
        setup.get_metrics().await,
        Metrics {
            requests: hashmap! {
                (rpc_method(), ALCHEMY_ETH_MAINNET_HOSTNAME.into()) => 1,
                (rpc_method(), ANKR_HOSTNAME.into()) => 1,
            },
            responses: hashmap! {
                (rpc_method(), ALCHEMY_ETH_MAINNET_HOSTNAME.into(), 200.into()) => 1,
                (rpc_method(), ANKR_HOSTNAME.into(), 400.into()) => 1,
            },
            inconsistent_responses: hashmap! {
                (rpc_method(), ALCHEMY_ETH_MAINNET_HOSTNAME.into()) => 1,
                (rpc_method(), ANKR_HOSTNAME.into()) => 1,
            },
            ..Default::default()
        }
    );
}

#[test]
fn candid_rpc_should_handle_already_known() {
    let setup = EvmRpcSetup::new().mock_api_keys();
    let result = setup
        .eth_send_raw_transaction(
            RpcServices::EthMainnet(Some(vec![
                EthMainnetService::Ankr,
                EthMainnetService::Cloudflare,
            ])),
            None,
            MOCK_TRANSACTION,
        )
        .mock_http_once(MockOutcallBuilder::new(
            200,
            r#"{"id":0,"jsonrpc":"2.0","result":"Ok"}"#,
        ))
        .mock_http_once(MockOutcallBuilder::new(
            200,
            r#"{"id":1,"jsonrpc":"2.0","error":{"code":-32000,"message":"already known"}}"#,
        ))
        .wait()
        .expect_consistent();
    assert_eq!(
        result,
        Ok(evm_rpc_types::SendRawTransactionStatus::Ok(Some(
            Hex32::from_str(MOCK_TRANSACTION_HASH).unwrap()
        )))
    );
    let rpc_method = || RpcMethod::EthSendRawTransaction.into();
    assert_eq!(
        setup.get_metrics(),
        Metrics {
            requests: hashmap! {
                (rpc_method(), ANKR_HOSTNAME.into()) => 1,
                (rpc_method(), CLOUDFLARE_HOSTNAME.into()) => 1,
            },
            responses: hashmap! {
                (rpc_method(), ANKR_HOSTNAME.into(), 200.into()) => 1,
                (rpc_method(), CLOUDFLARE_HOSTNAME.into(), 200.into()) => 1,
            },
            ..Default::default()
        }
    );
}

#[test]
fn candid_rpc_should_recognize_rate_limit() {
    let setup = EvmRpcSetup::new().mock_api_keys();
    let result = setup
        .eth_send_raw_transaction(
            RpcServices::EthMainnet(Some(vec![
                EthMainnetService::Ankr,
                EthMainnetService::Cloudflare,
            ])),
            None,
            MOCK_TRANSACTION,
        )
        .mock_http(MockOutcallBuilder::new(429, "(Rate limit error message)"))
        .wait()
        .expect_consistent();
    assert_eq!(
        result,
        Err(RpcError::HttpOutcallError(
            HttpOutcallError::InvalidHttpJsonRpcResponse {
                status: 429,
                body: "(Rate limit error message)".to_string(),
                parsing_error: None
            }
        ))
    );
    let rpc_method = || RpcMethod::EthSendRawTransaction.into();
    assert_eq!(
        setup.get_metrics(),
        Metrics {
            requests: hashmap! {
                (rpc_method(), ANKR_HOSTNAME.into()) => 1,
                (rpc_method(), CLOUDFLARE_HOSTNAME.into()) => 1,
            },
            responses: hashmap! {
                (rpc_method(), ANKR_HOSTNAME.into(), 429.into()) => 1,
                (rpc_method(), CLOUDFLARE_HOSTNAME.into(), 429.into()) => 1,
            },
            ..Default::default()
        }
    );
}

#[tokio::test]
async fn should_use_custom_response_size_estimate() {
    let setup = EvmRpcNonblockingSetup::new().await.mock_api_keys().await;
    let max_response_bytes = 1234;
    let expected_response = r#"{"id":0,"jsonrpc":"2.0","result":[{"address":"0xdac17f958d2ee523a2206206994597c13d831ec7","topics":["0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef","0x000000000000000000000000a9d1e08c7793af67e9d92fe308d5697fb81d3e43","0x00000000000000000000000078cccfb3d517cd4ed6d045e263e134712288ace2"],"data":"0x000000000000000000000000000000000000000000000000000000003b9c6433","blockNumber":"0x11dc77e","transactionHash":"0xf3ed91a03ddf964281ac7a24351573efd535b80fc460a5c2ad2b9d23153ec678","transactionIndex":"0x65","blockHash":"0xd5c72ad752b2f0144a878594faf8bd9f570f2f72af8e7f0940d3545a6388f629","logIndex":"0xe8","removed":false}]}"#;

    let mocks = MockHttpOutcallsBuilder::new()
        .given(
            JsonRpcRequestMatcher::with_method("eth_getLogs")
                .with_id(0_u64)
                .with_params(json!([{
                    "address" : ["0xdac17f958d2ee523a2206206994597c13d831ec7"],
                    "fromBlock": "latest",
                    "toBlock": "latest",
                }]))
                .with_max_response_bytes(max_response_bytes),
        )
        .respond_with(JsonRpcResponse::from(expected_response));

    let client = setup
        .client(mocks)
        .with_rpc_sources(RpcServices::EthMainnet(Some(vec![
            EthMainnetService::Cloudflare,
        ])))
        .with_response_size_estimate(max_response_bytes)
        .build();
    let response = client
        .get_logs(vec![address!("0xdAC17F958D2ee523a2206206994597C13D831ec7")])
        .send()
        .await
        .expect_consistent();
    assert_matches!(response, Ok(_));
}

#[tokio::test]
async fn should_use_fallback_public_url() {
    let setup = EvmRpcNonblockingSetup::new().await;
    let response = setup
        .client(
            MockHttpOutcallsBuilder::new()
                .given(get_transaction_count_request().with_url("https://rpc.ankr.com/eth"))
                .respond_with(get_transaction_count_response()),
        )
        .with_rpc_sources(RpcServices::EthMainnet(Some(vec![EthMainnetService::Ankr])))
        .build()
        .get_transaction_count((
            address!("0xdac17f958d2ee523a2206206994597c13d831ec7"),
            BlockNumberOrTag::Latest,
        ))
        .send()
        .await
        .expect_consistent()
        .unwrap();
    assert_eq!(response, U256::ONE);
}

#[tokio::test]
async fn should_insert_api_keys() {
    let setup = EvmRpcNonblockingSetup::with_args(InstallArgs {
        demo: Some(true),
        manage_api_keys: Some(vec![DEFAULT_CALLER_TEST_ID]),
        ..Default::default()
    })
    .await;
    let provider_id = 1;
    let api_keys = &[(provider_id, Some("test-api-key".to_string()))];
    setup
        .update_api_keys(api_keys, DEFAULT_CALLER_TEST_ID)
        .await;
    let response = setup
        .client(
            MockHttpOutcallsBuilder::new()
                .given(
                    get_transaction_count_request()
                        .with_url("https://rpc.ankr.com/eth/test-api-key"),
                )
                .respond_with(get_transaction_count_response()),
        )
        .with_rpc_sources(RpcServices::EthMainnet(Some(vec![EthMainnetService::Ankr])))
        .build()
        .get_transaction_count((
            address!("0xdac17f958d2ee523a2206206994597c13d831ec7"),
            BlockNumberOrTag::Latest,
        ))
        .send()
        .await
        .expect_consistent()
        .unwrap();
    assert_eq!(response, U256::ONE);
}

#[tokio::test]
async fn should_update_api_key() {
    let setup = EvmRpcNonblockingSetup::with_args(InstallArgs {
        demo: Some(true),
        manage_api_keys: Some(vec![DEFAULT_CALLER_TEST_ID]),
        ..Default::default()
    })
    .await;
    let provider_id = 1; // Ankr / mainnet
    let api_key = "test-api-key";

    let api_keys = &[(provider_id, Some(api_key.to_string()))];
    setup
        .update_api_keys(api_keys, DEFAULT_CALLER_TEST_ID)
        .await;
    let response = setup
        .client(
            MockHttpOutcallsBuilder::new()
                .given(
                    get_transaction_count_request()
                        .with_id(0_u64)
                        .with_url(&format!("https://rpc.ankr.com/eth/{api_key}")),
                )
                .respond_with(get_transaction_count_response().with_id(0_u64)),
        )
        .with_rpc_sources(RpcServices::EthMainnet(Some(vec![EthMainnetService::Ankr])))
        .build()
        .get_transaction_count((
            address!("0xdac17f958d2ee523a2206206994597c13d831ec7"),
            BlockNumberOrTag::Latest,
        ))
        .send()
        .await
        .expect_consistent()
        .unwrap();
    assert_eq!(response, U256::ONE);

    let api_keys = &[(provider_id, None)];
    setup
        .update_api_keys(api_keys, DEFAULT_CALLER_TEST_ID)
        .await;
    let response = setup
        .client(
            MockHttpOutcallsBuilder::new()
                .given(
                    get_transaction_count_request()
                        .with_id(1_u64)
                        .with_url("https://rpc.ankr.com/eth"),
                )
                .respond_with(get_transaction_count_response().with_id(1_u64)),
        )
        .with_rpc_sources(RpcServices::EthMainnet(Some(vec![EthMainnetService::Ankr])))
        .build()
        .get_transaction_count((
            address!("0xdac17f958d2ee523a2206206994597c13d831ec7"),
            BlockNumberOrTag::Latest,
        ))
        .send()
        .await
        .expect_consistent()
        .unwrap();
    assert_eq!(response, U256::ONE);
}

#[tokio::test]
async fn should_update_bearer_token() {
    let setup = EvmRpcNonblockingSetup::with_args(InstallArgs {
        demo: Some(true),
        manage_api_keys: Some(vec![DEFAULT_CALLER_TEST_ID]),
        ..Default::default()
    })
    .await;
    let provider_id = 8; // Alchemy / mainnet
    let api_key = "test-api-key";
    let api_keys = &[(provider_id, Some(api_key.to_string()))];
    setup
        .update_api_keys(api_keys, DEFAULT_CALLER_TEST_ID)
        .await;
    let response = setup
        .client(
            MockHttpOutcallsBuilder::new()
                .given(
                    get_transaction_count_request()
                        .with_url("https://eth-mainnet.g.alchemy.com/v2")
                        .with_request_headers(vec![
                            ("Content-Type", "application/json"),
                            ("Authorization", &format!("Bearer {api_key}")),
                        ]),
                )
                .respond_with(get_transaction_count_response()),
        )
        .with_rpc_sources(RpcServices::EthMainnet(Some(vec![
            EthMainnetService::Alchemy,
        ])))
        .build()
        .get_transaction_count((
            address!("0xdac17f958d2ee523a2206206994597c13d831ec7"),
            BlockNumberOrTag::Latest,
        ))
        .send()
        .await
        .expect_consistent()
        .unwrap();
    assert_eq!(response, U256::ONE);
}

#[test]
#[should_panic(expected = "You are not authorized")]
fn should_prevent_unauthorized_update_api_keys() {
    let setup = EvmRpcSetup::new();
    setup.update_api_keys(&[(0, Some("unauthorized-api-key".to_string()))]);
}

#[test]
#[should_panic(expected = "Trying to set API key for unauthenticated provider")]
fn should_prevent_unauthenticated_update_api_keys() {
    let setup = EvmRpcSetup::new();
    setup.as_controller().update_api_keys(&[(
        2, /* PublicNode / mainnet */
        Some("invalid-api-key".to_string()),
    )]);
}

#[test]
#[should_panic(expected = "Provider not found")]
fn should_prevent_unknown_provider_update_api_keys() {
    let setup = EvmRpcSetup::new();
    setup
        .as_controller()
        .update_api_keys(&[(5555, Some("unknown-provider-api-key".to_string()))]);
}

#[test]
fn should_get_nodes_in_subnet() {
    let setup = EvmRpcSetup::new();
    let nodes_in_subnet = setup.get_nodes_in_subnet();
    assert_eq!(nodes_in_subnet, 34);
}

#[test]
fn should_get_providers_and_get_service_provider_map_be_consistent() {
    let setup = EvmRpcSetup::new();
    let providers = setup.get_providers();
    let service_provider_map = setup.get_service_provider_map();
    assert_eq!(providers.len(), service_provider_map.len());

    for (service, provider_id) in service_provider_map {
        let found_provider = providers
            .iter()
            .find(|p| p.provider_id == provider_id)
            .unwrap();
        assert_eq!(found_provider.alias, Some(service));
    }
}

#[tokio::test]
async fn upgrade_should_keep_api_keys() {
    let setup = EvmRpcNonblockingSetup::with_args(InstallArgs {
        demo: Some(true),
        manage_api_keys: Some(vec![DEFAULT_CALLER_TEST_ID]),
        ..Default::default()
    })
    .await;
    let provider_id = 1; // Ankr / mainnet
    let api_key = "test-api-key";
    let api_keys = &[(provider_id, Some(api_key.to_string()))];
    setup
        .update_api_keys(api_keys, DEFAULT_CALLER_TEST_ID)
        .await;
    let response = setup
        .client(
            MockHttpOutcallsBuilder::new()
                .given(
                    get_transaction_count_request()
                        .with_url(&format!("https://rpc.ankr.com/eth/{api_key}")),
                )
                .respond_with(get_transaction_count_response()),
        )
        .with_rpc_sources(RpcServices::EthMainnet(Some(vec![EthMainnetService::Ankr])))
        .build()
        .get_transaction_count((
            address!("0xdac17f958d2ee523a2206206994597c13d831ec7"),
            BlockNumberOrTag::Latest,
        ))
        .send()
        .await
        .expect_consistent()
        .unwrap();
    assert_eq!(response, U256::ONE);

    setup.upgrade_canister(InstallArgs::default()).await;

    let response_post_upgrade = setup
        .client(
            MockHttpOutcallsBuilder::new()
                .given(
                    get_transaction_count_request()
                        .with_url(&format!("https://rpc.ankr.com/eth/{api_key}")),
                )
                .respond_with(get_transaction_count_response()),
        )
        .with_rpc_sources(RpcServices::EthMainnet(Some(vec![EthMainnetService::Ankr])))
        .build()
        .get_transaction_count((
            address!("0xdac17f958d2ee523a2206206994597c13d831ec7"),
            BlockNumberOrTag::Latest,
        ))
        .send()
        .await
        .expect_consistent()
        .unwrap();
    assert_eq!(response_post_upgrade, U256::ONE);
}

#[test]
fn upgrade_should_keep_demo() {
    let setup = EvmRpcSetup::with_args(InstallArgs {
        demo: Some(true),
        ..Default::default()
    });
    assert_eq!(
        setup
            .request_cost(
                RpcService::EthMainnet(EthMainnetService::PublicNode),
                r#"{"jsonrpc":"2.0","id":0,"method":"test"}"#,
                1000
            )
            .unwrap(),
        0_u32
    );
    setup.upgrade_canister(InstallArgs::default());
    assert_eq!(
        setup
            .request_cost(
                RpcService::EthMainnet(EthMainnetService::PublicNode),
                r#"{"jsonrpc":"2.0","id":0,"method":"test"}"#,
                1000
            )
            .unwrap(),
        0_u32
    );
}

#[test]
fn upgrade_should_change_demo() {
    let setup = EvmRpcSetup::with_args(InstallArgs {
        demo: Some(true),
        ..Default::default()
    });
    assert_eq!(
        setup
            .request_cost(
                RpcService::EthMainnet(EthMainnetService::PublicNode),
                r#"{"jsonrpc":"2.0","id":0,"method":"test"}"#,
                1000
            )
            .unwrap(),
        0_u32
    );
    setup.upgrade_canister(InstallArgs {
        demo: Some(false),
        ..Default::default()
    });
    assert_ne!(
        setup
            .request_cost(
                RpcService::EthMainnet(EthMainnetService::PublicNode),
                r#"{"jsonrpc":"2.0","id":0,"method":"test"}"#,
                1000
            )
            .unwrap(),
        0_u32
    );
}

#[test]
fn upgrade_should_keep_manage_api_key_principals() {
    let authorized_caller = ADDITIONAL_TEST_ID;
    let setup = EvmRpcSetup::with_args(InstallArgs {
        manage_api_keys: Some(vec![authorized_caller]),
        ..Default::default()
    });
    setup.upgrade_canister(InstallArgs {
        manage_api_keys: None,
        ..Default::default()
    });
    setup
        .as_caller(authorized_caller)
        .update_api_keys(&[(0, Some("authorized-api-key".to_string()))]);
}

#[test]
#[should_panic(expected = "You are not authorized")]
fn upgrade_should_change_manage_api_key_principals() {
    let deauthorized_caller = ADDITIONAL_TEST_ID;
    let setup = EvmRpcSetup::with_args(InstallArgs {
        manage_api_keys: Some(vec![deauthorized_caller]),
        ..Default::default()
    });
    setup.upgrade_canister(InstallArgs {
        manage_api_keys: Some(vec![]),
        ..Default::default()
    });
    setup
        .as_caller(deauthorized_caller)
        .update_api_keys(&[(0, Some("unauthorized-api-key".to_string()))]);
}

#[test]
fn should_reject_http_request_in_replicated_mode() {
    let request = HttpRequest {
        method: "".to_string(),
        url: "/nonexistent".to_string(),
        headers: vec![],
        body: serde_bytes::ByteBuf::new(),
    };
    assert_matches!(
        EvmRpcSetup::new()
        .env
        .update_call(
            EvmRpcSetup::new().canister_id,
            Principal::anonymous(),
            "http_request",
            Encode!(&request).unwrap(),
        ),
        Err(e) if e.error_code == ErrorCode::CanisterCalledTrap && e.reject_message.contains("Update call rejected")
    );
}

#[test]
fn should_retrieve_logs() {
    let setup = EvmRpcSetup::with_args(InstallArgs {
        demo: None,
        manage_api_keys: None,
        ..Default::default()
    });
    assert_eq!(setup.http_get_logs("DEBUG"), vec![]);
    assert_eq!(setup.http_get_logs("INFO"), vec![]);

    let setup = setup.mock_api_keys();

    assert_eq!(setup.http_get_logs("DEBUG"), vec![]);
    assert!(setup.http_get_logs("INFO")[0]
        .message
        .contains("Updating API keys"));
}

#[tokio::test]
async fn should_retry_when_response_too_large() {
    let setup = EvmRpcNonblockingSetup::new().await.mock_api_keys().await;

    let rpc_services = RpcServices::EthMainnet(Some(vec![EthMainnetService::Cloudflare]));

    // around 600 bytes per log
    // we need at least 3334 logs to reach the 2MB limit
    let response_body = multi_logs_for_single_transaction(3_500);
    let max_response_bytes = iter::once(1_u64)
        .chain((1..=10).map(|i| 1024_u64 << i))
        .chain(iter::once(2_000_000_u64));

    let mut mocks = MockHttpOutcallsBuilder::new();
    for (id, max_response_bytes) in max_response_bytes.enumerate() {
        mocks = mocks
            .given(
                JsonRpcRequestMatcher::with_method("eth_getLogs")
                    .with_id(id as u64)
                    .with_params(json!([{
                        "address" : ["0xdac17f958d2ee523a2206206994597c13d831ec7"],
                        "fromBlock": "latest",
                        "toBlock": "latest",
                    }]))
                    .with_max_response_bytes(max_response_bytes),
            )
            .respond_with(JsonRpcResponse::from(&response_body).with_id(id as u64));
    }

    let response = setup
        .client(mocks)
        .with_rpc_sources(rpc_services.clone())
        .with_response_size_estimate(1)
        .build()
        .get_logs(vec![address!("0xdAC17F958D2ee523a2206206994597C13D831ec7")])
        .send()
        .await
        .expect_consistent();

    assert_matches!(
        response,
        Err(RpcError::HttpOutcallError(HttpOutcallError::IcError { code, message }))
        if code == LegacyRejectionCode::SysFatal && message.contains("body exceeds size limit")
    );

    let response_body = multi_logs_for_single_transaction(1_000);
    let max_response_bytes = iter::once(1_u64).chain((1..=10).map(|i| 1024_u64 << i));

    let mut mocks = MockHttpOutcallsBuilder::new();
    for (id, max_response_bytes) in max_response_bytes.enumerate() {
        mocks = mocks
            .given(
                JsonRpcRequestMatcher::with_method("eth_getLogs")
                    .with_id(id as u64 + 12)
                    .with_params(json!([{
                        "address" : ["0xdac17f958d2ee523a2206206994597c13d831ec7"],
                        "fromBlock": "latest",
                        "toBlock": "latest",
                    }]))
                    .with_max_response_bytes(max_response_bytes),
            )
            .respond_with(JsonRpcResponse::from(&response_body).with_id(id as u64 + 12));
    }

    let response = setup
        .client(mocks)
        .with_rpc_sources(rpc_services.clone())
        .with_response_size_estimate(1)
        .build()
        .get_logs(vec![address!("0xdAC17F958D2ee523a2206206994597C13D831ec7")])
        .send()
        .await
        .expect_consistent();

    assert_matches!(
        response,
        Ok(logs) if logs.len() == 1_000
    );
}

#[tokio::test]
async fn should_have_different_request_ids_when_retrying_because_response_too_big() {
    let setup = EvmRpcNonblockingSetup::new().await.mock_api_keys().await;

    let mocks = MockHttpOutcallsBuilder::new()
        .given(
            get_transaction_count_request()
                .with_id(0_u64)
                .with_max_response_bytes(1_u64),
        )
        .respond_with(get_transaction_count_response().with_id(0_u64))
        .given(
            get_transaction_count_request()
                .with_id(1_u64)
                .with_max_response_bytes(2048_u64),
        )
        .respond_with(get_transaction_count_response().with_id(1_u64));

    let response = setup
        .client(mocks)
        .with_rpc_sources(RpcServices::EthMainnet(Some(vec![
            EthMainnetService::Cloudflare,
        ])))
        .with_response_size_estimate(1)
        .build()
        .get_transaction_count((
            address!("0xdac17f958d2ee523a2206206994597c13d831ec7"),
            BlockNumberOrTag::Latest,
        ))
        .send()
        .await
        .expect_consistent();

    assert_eq!(response, Ok(U256::ONE));

    let rpc_method = || RpcMethod::EthGetTransactionCount.into();
    assert_eq!(
        setup.get_metrics().await,
        Metrics {
            requests: hashmap! {
                (rpc_method(), CLOUDFLARE_HOSTNAME.into()) => 2,
            },
            responses: hashmap! {
                (rpc_method(), CLOUDFLARE_HOSTNAME.into(), 200.into()) => 1,
            },
            err_max_response_size_exceeded: hashmap! {
                (rpc_method(), CLOUDFLARE_HOSTNAME.into()) => 1,
            },
            ..Default::default()
        }
    );
}

#[tokio::test]
async fn should_fail_when_response_id_inconsistent_with_request_id() {
    let setup = EvmRpcNonblockingSetup::new().await.mock_api_keys().await;

    let request_id = 0_u64;
    let response_id = 1_u64;
    assert_ne!(request_id, response_id);

    let error = setup
        .client(
            MockHttpOutcallsBuilder::new()
                .given(get_transaction_count_request().with_id(request_id))
                .respond_with(get_transaction_count_response().with_id(response_id)),
        )
        .with_rpc_sources(RpcServices::EthMainnet(Some(vec![
            EthMainnetService::Cloudflare,
        ])))
        .build()
        .get_transaction_count((
            address!("0xdac17f958d2ee523a2206206994597c13d831ec7"),
            BlockNumberOrTag::Latest,
        ))
        .send()
        .await
        .expect_consistent()
        .expect_err("should fail when ID mismatch");

    assert!(
        error
            .to_string()
            .to_ascii_lowercase()
            .contains("unexpected identifier"),
        "unexpected error: {error}"
    );
}

#[tokio::test]
async fn should_log_request() {
    fn mock_request() -> JsonRpcRequestMatcher {
        JsonRpcRequestMatcher::with_method("eth_feeHistory")
            .with_params(json!(["0x3", "latest", []]))
            .with_id(0_u64)
    }

    fn mock_response() -> JsonRpcResponse {
        JsonRpcResponse::from(json!({
            "id" : 0,
            "jsonrpc" : "2.0",
            "result" : {
                "oldestBlock" : "0x11e57f5",
                "baseFeePerGas" : ["0x9cf6c61b9", "0x97d853982", "0x9ba55a0b0", "0x9543bf98d"],
                "reward" : [["0x0123"]]
            }
        }))
    }

    let setup = EvmRpcNonblockingSetup::new().await.mock_api_keys().await;

    let mocks = MockHttpOutcallsBuilder::new()
        .given(mock_request())
        .respond_with(mock_response());

    let response = setup
        .client(mocks)
        .with_rpc_sources(RpcServices::EthMainnet(Some(vec![
            EthMainnetService::Alchemy,
        ])))
        .build()
        .fee_history((3_u64, BlockNumberOrTag::Latest))
        .send()
        .await
        .expect_consistent()
        .unwrap();

    assert_eq!(
        response,
        alloy_rpc_types::FeeHistory {
            oldest_block: 0x11e57f5_u64,
            base_fee_per_gas: vec![0x9cf6c61b9_u128, 0x97d853982, 0x9ba55a0b0, 0x9543bf98d],
            gas_used_ratio: vec![],
            reward: Some(vec![vec![0x0123_u128]]),
            base_fee_per_blob_gas: vec![],
            blob_gas_used_ratio: vec![],
        }
    );

    let logs = setup.http_get_logs("TRACE_HTTP").await;
    assert_eq!(logs.len(), 2, "Unexpected amount of logs {logs:?}");
    assert!(logs[0].message.contains("JSON-RPC request with id `0` to eth-mainnet.g.alchemy.com: JsonRpcRequest { jsonrpc: V2, method: \"eth_feeHistory\""));
    assert!(logs[1].message.contains("response for request with id `0`. Response with status 200 OK: JsonRpcResponse { jsonrpc: V2, id: Number(0), result: Ok(FeeHistory"));
}

#[tokio::test]
async fn should_change_default_provider_when_one_keeps_failing() {
    let setup = EvmRpcNonblockingSetup::new().await.mock_api_keys().await;

    let response = setup
        .client(
            MockHttpOutcallsBuilder::new()
                .given(
                    get_transaction_count_request()
                        .with_id(0_u64)
                        .with_host(ANKR_HOSTNAME),
                )
                .respond_with(get_transaction_count_response().with_id(0_u64))
                .given(
                    get_transaction_count_request()
                        .with_id(1_u64)
                        .with_host(BLOCKPI_ETH_HOSTNAME),
                )
                .respond_with(CanisterHttpReply::with_status(500).with_body("Error!"))
                .given(
                    get_transaction_count_request()
                        .with_id(2_u64)
                        .with_host(PUBLICNODE_ETH_MAINNET_HOSTNAME),
                )
                .respond_with(get_transaction_count_response().with_id(2_u64)),
        )
        .with_rpc_sources(RpcServices::EthMainnet(None))
        .with_consensus_strategy(ConsensusStrategy::Threshold {
            total: Some(3),
            min: 2,
        })
        .build()
        .get_transaction_count((
            address!("0xdac17f958d2ee523a2206206994597c13d831ec7"),
            BlockNumberOrTag::Latest,
        ))
        .send()
        .await
        .expect_consistent()
        .unwrap();
    assert_eq!(response, U256::ONE);

    let response = setup
        .client(
            MockHttpOutcallsBuilder::new()
                .given(
                    get_transaction_count_request()
                        .with_id(3_u64)
                        .with_host(ALCHEMY_ETH_MAINNET_HOSTNAME),
                )
                .respond_with(get_transaction_count_response().with_id(3_u64))
                .given(
                    get_transaction_count_request()
                        .with_id(4_u64)
                        .with_host(ANKR_HOSTNAME),
                )
                .respond_with(get_transaction_count_response().with_id(4_u64)),
        )
        .with_rpc_sources(RpcServices::EthMainnet(Some(vec![
            EthMainnetService::Ankr,
            EthMainnetService::Alchemy,
        ])))
        .with_consensus_strategy(ConsensusStrategy::Equality)
        .build()
        .get_transaction_count((
            address!("0xdac17f958d2ee523a2206206994597c13d831ec7"),
            BlockNumberOrTag::Latest,
        ))
        .send()
        .await
        .expect_consistent()
        .unwrap();
    assert_eq!(response, U256::ONE);

    let response = setup
        .client(
            MockHttpOutcallsBuilder::new()
                .given(
                    get_transaction_count_request()
                        .with_id(5_u64)
                        .with_host(ALCHEMY_ETH_MAINNET_HOSTNAME),
                )
                .respond_with(get_transaction_count_response().with_id(5_u64))
                .given(
                    get_transaction_count_request()
                        .with_id(6_u64)
                        .with_host(ANKR_HOSTNAME),
                )
                .respond_with(get_transaction_count_response().with_id(6_u64))
                .given(
                    get_transaction_count_request()
                        .with_id(7_u64)
                        .with_host(PUBLICNODE_ETH_MAINNET_HOSTNAME),
                )
                .respond_with(get_transaction_count_response().with_id(7_u64)),
        )
        .with_rpc_sources(RpcServices::EthMainnet(None))
        .with_consensus_strategy(ConsensusStrategy::Threshold {
            total: Some(3),
            min: 2,
        })
        .build()
        .get_transaction_count((
            address!("0xdac17f958d2ee523a2206206994597c13d831ec7"),
            BlockNumberOrTag::Latest,
        ))
        .send()
        .await
        .expect_consistent()
        .unwrap();
    assert_eq!(response, U256::ONE);
}

fn get_transaction_count_request() -> JsonRpcRequestMatcher {
    JsonRpcRequestMatcher::with_method("eth_getTransactionCount")
        .with_params(json!([
            "0xdac17f958d2ee523a2206206994597c13d831ec7",
            "latest"
        ]))
        .with_id(0_u64)
}

fn get_transaction_count_response() -> JsonRpcResponse {
    JsonRpcResponse::from(json!({ "jsonrpc" : "2.0", "id" : 0, "result" : "0x1" }))
}

pub fn multi_logs_for_single_transaction(num_logs: usize) -> serde_json::Value {
    let mut logs = Vec::with_capacity(num_logs);
    for log_index in 0..num_logs {
        let mut log = single_log();
        log.log_index = Some(log_index.into());
        logs.push(log);
    }
    json!({"jsonrpc":"2.0","result": logs,"id":0})
}

fn single_log() -> ethers_core::types::Log {
    let json_value = json!({
       "address": "0xb44b5e756a894775fc32eddf3314bb1b1944dc34",
        "blockHash": "0xc5e46f4f529cfd2abf1c5dfaad4c4ea8d297795c8632b5056bc6f9eed1f5a7eb",
        "blockNumber": "0x47b133",
        "data": "0x00000000000000000000000000000000000000000000000000038d7ea4c68000",
        "logIndex": "0x2e",
        "removed": false,
        "topics": [
            "0x257e057bb61920d8d0ed2cb7b720ac7f9c513cd1110bc9fa543079154f45f435",
            "0x000000000000000000000000c8a1bc416c8498af8dc03b253a513d379d3e4ee8",
            "0x1d9facb184cbe453de4841b6b9d9cc95bfc065344e485789b550544529020000"
        ],
        "transactionHash": "0x42826e03a51e735a1adc6ed026796d9044d6942c8de660017289cdc4787f3983",
        "transactionIndex": "0x20"
    });
    serde_json::from_value(json_value).expect("BUG: invalid log entry")
}

fn json_rpc_sequential_id<const N: usize>(response: serde_json::Value) -> [serde_json::Value; N] {
    let first_id = response["id"].as_u64().expect("missing request ID");
    let mut requests = Vec::with_capacity(N);
    requests.push(response.clone());
    for i in 1..N {
        let mut next_request = response.clone();
        let new_id = first_id + i as u64;
        *next_request.get_mut("id").unwrap() = serde_json::Value::Number(new_id.into());
        requests.push(next_request);
    }
    requests.try_into().unwrap()
}

fn add_offset_json_rpc_id(inputs: &mut [serde_json::Value], offset: u64) {
    for input in inputs {
        let current = input["id"].as_u64().expect("missing request ID");
        let new = current + offset;
        input["id"] = serde_json::Value::Number(new.into());
    }
}

pub struct TestCase<Req, Res> {
    pub request: Req,
    pub raw_body: serde_json::Value,
    pub expected: Res,
}

mod mock;

use std::{marker::PhantomData, rc::Rc, time::Duration};

use assert_matches::assert_matches;
use candid::{CandidType, Decode, Encode, Nat};
use ic_base_types::{CanisterId, PrincipalId};
use ic_cdk::api::management_canister::http_request::{
    CanisterHttpRequestArgument, HttpHeader, HttpMethod, HttpResponse as OutCallHttpResponse,
    TransformArgs, TransformContext, TransformFunc,
};
use ic_ic00_types::CanisterSettingsArgsBuilder;
use ic_state_machine_tests::{
    CanisterHttpResponsePayload, Cycles, IngressState, IngressStatus, MessageId, PayloadBuilder,
    StateMachine, StateMachineBuilder, WasmResult,
};
use ic_test_utilities_load_wasm::load_wasm;
use serde::de::DeserializeOwned;

use evm_rpc::*;
use mock::*;

const DEFAULT_CALLER_TEST_ID: u64 = 10352385;
const DEFAULT_CONTROLLER_TEST_ID: u64 = 10352386;

const INITIAL_CYCLES: u128 = 100_000_000_000_000_000;

const MAX_TICKS: usize = 10;

const MOCK_REQUEST_URL: &str = "https://cloudflare-eth.com";
const MOCK_REQUEST_PAYLOAD: &str = r#"{"id":1,"jsonrpc":"2.0","method":"eth_gasPrice"}"#;
const MOCK_REQUEST_RESPONSE: &str = r#"{"id":1,"jsonrpc":"2.0","result":"0x00112233"}"#;
const MOCK_REQUEST_RESPONSE_BYTES: u64 = 1000;

fn evm_rpc_wasm() -> Vec<u8> {
    load_wasm(std::env::var("CARGO_MANIFEST_DIR").unwrap(), "evm_rpc", &[])
}

fn assert_reply(result: WasmResult) -> Vec<u8> {
    match result {
        WasmResult::Reply(bytes) => bytes,
        result => {
            panic!("Expected a successful reply, got {}", result)
        }
    }
}

#[derive(Clone)]
pub struct EvmRpcSetup {
    pub env: Rc<StateMachine>,
    pub caller: PrincipalId,
    pub controller: PrincipalId,
    pub canister_id: CanisterId,
}

impl Default for EvmRpcSetup {
    fn default() -> Self {
        Self::new()
    }
}

impl EvmRpcSetup {
    pub fn new() -> Self {
        let env = Rc::new(
            StateMachineBuilder::new()
                .with_default_canister_range()
                .build(),
        );

        let controller = PrincipalId::new_user_test_id(DEFAULT_CONTROLLER_TEST_ID);
        let canister_id = env.create_canister_with_cycles(
            None,
            Cycles::new(INITIAL_CYCLES),
            Some(
                CanisterSettingsArgsBuilder::default()
                    .with_controller(controller)
                    .build(),
            ),
        );
        env.install_existing_canister(canister_id, evm_rpc_wasm(), Encode!(&()).unwrap())
            .unwrap();

        let caller = PrincipalId::new_user_test_id(DEFAULT_CALLER_TEST_ID);

        Self {
            env,
            caller,
            controller,
            canister_id,
        }
    }

    /// Shorthand for deriving an `EvmRpcSetup` with the caller as the canister controller.
    pub fn as_controller(&self) -> Self {
        let mut setup = self.clone();
        setup.caller = self.controller;
        setup
    }

    /// Shorthand for deriving an `EvmRpcSetup` with an anonymous caller.
    pub fn as_anonymous(&self) -> Self {
        let mut setup = self.clone();
        setup.caller = PrincipalId::new_anonymous();
        setup
    }

    /// Shorthand for deriving an `EvmRpcSetup` with an arbitrary caller.
    pub fn as_caller(&self, id: PrincipalId) -> Self {
        let mut setup = self.clone();
        setup.caller = id;
        setup
    }

    fn call_update<R: CandidType + DeserializeOwned>(
        &self,
        method: &str,
        input: Vec<u8>,
    ) -> CallFlow<R> {
        CallFlow::from_update(self.clone(), method, input)
    }

    fn call_query<R: CandidType + DeserializeOwned>(&self, method: &str, input: Vec<u8>) -> R {
        Decode!(
            &assert_reply(
                self.env
                    .query_as(self.caller, self.canister_id, method, input,)
                    .unwrap_or_else(|err| panic!(
                        "error during query call to `{}()`: {}",
                        method, err
                    ))
            ),
            R
        )
        .unwrap()
    }

    pub fn tick_until_http_request(&self) {
        for _ in 0..MAX_TICKS {
            if !self.env.canister_http_request_contexts().is_empty() {
                break;
            }
            self.env.tick();
            self.env.advance_time(Duration::from_nanos(1));
        }
    }

    pub fn authorize(&self, principal: &PrincipalId, auth: Auth) -> CallFlow<()> {
        self.call_update("authorize", Encode!(&principal.0, &auth).unwrap())
    }

    pub fn deauthorize(&self, principal: &PrincipalId, auth: Auth) -> CallFlow<()> {
        self.call_update("deauthorize", Encode!(&principal.0, &auth).unwrap())
    }

    pub fn get_providers(&self) -> Vec<ProviderView> {
        self.call_query("get_providers", Encode!().unwrap())
    }

    pub fn register_provider(&self, args: RegisterProviderArgs) -> CallFlow<u64> {
        self.call_update("register_provider", Encode!(&args).unwrap())
    }

    pub fn authorize_caller(&self, auth: Auth) -> CallFlow<()> {
        self.as_controller().authorize(&self.caller, auth)
    }

    pub fn deauthorize_caller(&self, auth: Auth) -> CallFlow<()> {
        self.as_controller().deauthorize(&self.caller, auth)
    }

    pub fn request_cost(
        &self,
        source: Source,
        json_rpc_payload: &str,
        max_response_bytes: u64,
    ) -> Nat {
        self.call_query(
            "request_cost",
            Encode!(&source, &json_rpc_payload, &max_response_bytes).unwrap(),
        )
    }

    pub fn request(
        &self,
        source: Source,
        json_rpc_payload: &str,
        max_response_bytes: u64,
    ) -> CallFlow<RpcResult<String>> {
        self.call_update(
            "request",
            Encode!(&source, &json_rpc_payload, &max_response_bytes).unwrap(),
        )
    }
}

pub struct CallFlow<R> {
    setup: EvmRpcSetup,
    method: String,
    message_id: MessageId,
    phantom: PhantomData<R>,
}

impl<R: CandidType + DeserializeOwned> CallFlow<R> {
    pub fn from_update(setup: EvmRpcSetup, method: &str, input: Vec<u8>) -> Self {
        let message_id = setup
            .env
            .send_ingress(setup.caller, setup.canister_id, method, input);
        CallFlow::new(setup, method, message_id)
    }

    pub fn new(setup: EvmRpcSetup, method: impl ToString, message_id: MessageId) -> Self {
        Self {
            setup,
            method: method.to_string(),
            message_id,
            phantom: Default::default(),
        }
    }

    pub fn mock_http(self, mock: impl Into<MockOutcall>) -> Self {
        let mock = mock.into();
        assert_eq!(self.setup.env.canister_http_request_contexts().len(), 0);
        self.setup.tick_until_http_request();
        match self.setup.env.ingress_status(&self.message_id) {
            IngressStatus::Known { state, .. } if state != IngressState::Processing => return self,
            _ => (),
        }
        let contexts = self.setup.env.canister_http_request_contexts();
        let (id, context) = contexts.first_key_value().expect("no pending HTTP request");

        mock.assert_matches(&CanisterHttpRequestArgument {
            url: context.url.clone(),
            max_response_bytes: context.max_response_bytes.map(|n| n.get()),
            // Convert HTTP method type by name
            method: serde_json::from_str(
                &serde_json::to_string(&context.http_method)
                    .unwrap()
                    .to_lowercase(),
            )
            .unwrap(),
            headers: context
                .headers
                .iter()
                .map(|h| HttpHeader {
                    name: h.name.clone(),
                    value: h.value.clone(),
                })
                .collect(),
            body: context.body.clone(),
            transform: context.transform.clone().map(|t| TransformContext {
                context: t.context,
                function: TransformFunc::new(self.setup.canister_id.get().0, t.method_name),
            }),
        });
        let mut response = OutCallHttpResponse {
            status: mock.response.status,
            headers: mock.response.headers,
            body: mock.response.body,
        };
        if let Some(transform) = &context.transform {
            let transform_args = TransformArgs {
                response,
                context: transform.context.to_vec(),
            };
            response = Decode!(
                &assert_reply(
                    self.setup
                        .env
                        .execute_ingress(
                            self.setup.canister_id,
                            transform.method_name.clone(),
                            Encode!(&transform_args).unwrap(),
                        )
                        .expect("failed to query transform HTTP response")
                ),
                OutCallHttpResponse
            )
            .unwrap();
        }
        let http_response = CanisterHttpResponsePayload {
            status: response.status.0.try_into().unwrap(),
            headers: response
                .headers
                .into_iter()
                .map(|h| ic_ic00_types::HttpHeader {
                    name: h.name,
                    value: h.value,
                })
                .collect(),
            body: response.body,
        };
        let payload = PayloadBuilder::new().http_response(*id, &http_response);
        self.setup.env.execute_payload(payload);

        self
    }

    pub fn wait(self) -> R {
        Decode!(
            &assert_reply(
                self.setup
                    .env
                    .await_ingress(self.message_id, MAX_TICKS)
                    .unwrap_or_else(|err| {
                        panic!("error during update call to `{}()`: {}", self.method, err)
                    })
            ),
            R
        )
        .unwrap()
    }
}

#[test]
fn should_register_provider() {
    let setup = EvmRpcSetup::new();
    setup.authorize_caller(Auth::RegisterProvider);

    assert_eq!(
        setup
            .get_providers()
            .into_iter()
            .map(|p| (p.chain_id, p.hostname))
            .collect::<Vec<_>>(),
        get_default_providers()
            .into_iter()
            .map(|p| (p.chain_id, p.hostname))
            .collect::<Vec<_>>()
    );
    let n_providers = 2;
    let a_id = setup
        .register_provider(RegisterProviderArgs {
            chain_id: 1,
            hostname: ANKR_HOSTNAME.to_string(),
            credential_path: "".to_string(),
            credential_headers: None,
            cycles_per_call: 0,
            cycles_per_message_byte: 0,
        })
        .wait();
    let b_id = setup
        .register_provider(RegisterProviderArgs {
            chain_id: 5,
            hostname: CLOUDFLARE_ETH_HOSTNAME.to_string(),
            credential_path: "/test-path".to_string(),
            credential_headers: Some(vec![HttpHeader {
                name: "Test-Authorization".to_string(),
                value: "---".to_string(),
            }]),
            cycles_per_call: 0,
            cycles_per_message_byte: 0,
        })
        .wait();
    assert_eq!(a_id + 1, b_id);
    let providers = setup.get_providers();
    assert_eq!(providers.len(), get_default_providers().len() + n_providers);
    let first_new_id = (providers.len() - n_providers) as u64;
    assert_eq!(
        providers[providers.len() - n_providers..],
        vec![
            ProviderView {
                provider_id: first_new_id,
                owner: setup.caller.0,
                chain_id: 1,
                hostname: ANKR_HOSTNAME.to_string(),
                cycles_per_call: 0,
                cycles_per_message_byte: 0,
                primary: false,
            },
            ProviderView {
                provider_id: first_new_id + 1,
                owner: setup.caller.0,
                chain_id: 5,
                hostname: CLOUDFLARE_ETH_HOSTNAME.to_string(),
                cycles_per_call: 0,
                cycles_per_message_byte: 0,
                primary: false,
            }
        ]
    )
}

fn mock_request(builder_fn: impl Fn(MockOutcallBuilder) -> MockOutcallBuilder) {
    let setup = EvmRpcSetup::new();
    setup.authorize_caller(Auth::FreeRpc);

    assert_matches!(
        setup
            .request(
                Source::Custom {
                    url: MOCK_REQUEST_URL.to_string(),
                    headers: Some(vec![HttpHeader {
                        name: "Custom".to_string(),
                        value: "Value".to_string(),
                    }]),
                },
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
    mock_request(|builder| builder.with_method(HttpMethod::POST))
}

#[test]
fn mock_request_should_succeed_with_request_headers() {
    mock_request(|builder| {
        builder.with_request_headers(vec![
            (CONTENT_TYPE_HEADER, CONTENT_TYPE_VALUE),
            ("Custom", "Value"),
        ])
    })
}

#[test]
fn mock_request_should_succeed_with_request_body() {
    mock_request(|builder| builder.with_request_body(MOCK_REQUEST_PAYLOAD))
}

#[test]
fn mock_request_should_succeed_with_all() {
    mock_request(|builder| {
        builder
            .with_url(MOCK_REQUEST_URL)
            .with_method(HttpMethod::POST)
            .with_request_headers(vec![
                (CONTENT_TYPE_HEADER, CONTENT_TYPE_VALUE),
                ("Custom", "Value"),
            ])
            .with_request_body(MOCK_REQUEST_PAYLOAD)
    })
}

#[test]
#[should_panic(expected = "assertion failed: `(left == right)`")]
fn mock_request_should_fail_with_url() {
    mock_request(|builder| builder.with_url("https://not-the-url.com"))
}

#[test]
#[should_panic(expected = "assertion failed: `(left == right)`")]
fn mock_request_should_fail_with_method() {
    mock_request(|builder| builder.with_method(HttpMethod::GET))
}

#[test]
#[should_panic(expected = "assertion failed: `(left == right)`")]
fn mock_request_should_fail_with_request_headers() {
    mock_request(|builder| builder.with_request_headers(vec![("Custom", "NotValue")]))
}

#[test]
#[should_panic(expected = "assertion failed: `(left == right)`")]
fn mock_request_should_fail_with_request_body() {
    mock_request(|builder| builder.with_request_body(r#"{"different":"body"}"#))
}

#[test]
fn should_canonicalize_json_response() {
    let setup = EvmRpcSetup::new();
    setup.authorize_caller(Auth::FreeRpc);
    let responses = [
        r#"{"id":1,"jsonrpc":"2.0","result":"0x00112233"}"#,
        r#"{"result":"0x00112233","id":1,"jsonrpc":"2.0"}"#,
        r#"{"result":"0x00112233","jsonrpc":"2.0","id":1}"#,
    ]
    .into_iter()
    .map(|response| {
        setup
            .request(
                Source::Custom {
                    url: MOCK_REQUEST_URL.to_string(),
                    headers: None,
                },
                MOCK_REQUEST_PAYLOAD,
                MOCK_REQUEST_RESPONSE_BYTES,
            )
            .mock_http(MockOutcallBuilder::new(200, response))
            .wait()
    })
    .collect::<Vec<_>>();
    assert!(responses.windows(2).all(|w| w[0] == w[1]));
}

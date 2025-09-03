use crate::mock_http_runtime::mock::MockHttpOutcalls;
use crate::mock_http_runtime::MockHttpRuntime;
use crate::{DEFAULT_CALLER_TEST_ID, DEFAULT_CONTROLLER_TEST_ID, INITIAL_CYCLES, MOCK_API_KEY};
use candid::{Encode, Principal};
use evm_rpc::providers::PROVIDERS;
use evm_rpc::types::{ProviderId, RpcAccess};
use evm_rpc_client::{ClientBuilder, EvmRpcClient};
use evm_rpc_types::InstallArgs;
use ic_cdk::api::management_canister::main::CanisterId;
use ic_management_canister_types::CanisterSettings;
use pocket_ic::{nonblocking, PocketIcBuilder};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct EvmRpcNonblockingSetup {
    pub env: Arc<nonblocking::PocketIc>,
    pub caller: Principal,
    pub controller: Principal,
    pub canister_id: CanisterId,
    pub mocks: MockHttpOutcalls,
}

impl EvmRpcNonblockingSetup {
    pub async fn new() -> Self {
        Self::with_args(InstallArgs {
            demo: Some(true),
            ..Default::default()
        })
        .await
    }

    pub async fn with_args(args: InstallArgs) -> Self {
        // The `with_fiduciary_subnet` setup below requires that `nodes_in_subnet`
        // setting (part of InstallArgs) to be set appropriately. Otherwise
        // http outcall will fail due to insufficient cycles, even when `demo` is
        // enabled (which is the default above).
        //
        // As of writing, the default value of `nodes_in_subnet` is 34, which is
        // also the node count in fiduciary subnet.
        let pocket_ic = PocketIcBuilder::new()
            .with_fiduciary_subnet()
            .build_async()
            .await;
        let env = Arc::new(pocket_ic);

        let controller = DEFAULT_CONTROLLER_TEST_ID;
        let canister_id = env
            .create_canister_with_settings(
                None,
                Some(CanisterSettings {
                    controllers: Some(vec![controller]),
                    ..CanisterSettings::default()
                }),
            )
            .await;
        env.add_cycles(canister_id, INITIAL_CYCLES).await;
        env.install_canister(
            canister_id,
            crate::evm_rpc_wasm(),
            Encode!(&args).unwrap(),
            Some(controller),
        )
        .await;

        let caller = DEFAULT_CALLER_TEST_ID;

        Self {
            env,
            caller,
            controller,
            canister_id,
            mocks: MockHttpOutcalls::default(),
        }
    }

    pub fn with_http_mocks(self, mocks: impl Into<MockHttpOutcalls>) -> Self {
        Self {
            mocks: mocks.into(),
            ..self
        }
    }

    pub fn client(&self) -> ClientBuilder<MockHttpRuntime> {
        EvmRpcClient::builder(self.new_mock_http_runtime(), self.canister_id)
    }

    fn new_mock_http_runtime(&self) -> MockHttpRuntime {
        MockHttpRuntime {
            env: self.env.clone(),
            caller: self.caller,
            mocks: Mutex::new(self.mocks.clone()),
        }
    }

    pub async fn update_api_keys(&self, api_keys: &[(ProviderId, Option<String>)]) {
        self.env
            .update_call(
                self.canister_id,
                self.controller,
                "updateApiKeys",
                Encode!(&api_keys).expect("Failed to encode arguments."),
            )
            .await
            .expect("BUG: Failed to call updateApiKeys");
    }

    pub async fn mock_api_keys(self) -> Self {
        self.clone()
            .update_api_keys(
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
            )
            .await;
        self
    }
}

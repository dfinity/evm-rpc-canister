use crate::constants::{COLLATERAL_CYCLES_PER_NODE, CONTENT_TYPE_VALUE};
use crate::types::{ApiKey, LogFilter, Metrics, OverrideProvider, ProviderId};
use candid::Principal;
use canhttp::{
    map_ic_http_response, CyclesAccounting, CyclesAccountingError,
    DefaultRequestCyclesCostEstimator, CyclesChargingPolicy, FullBytes, HttpRequestFilter,
};
use evm_rpc_types::{HttpOutcallError, ProviderError, RpcError};
use http::header::CONTENT_TYPE;
use http::HeaderValue;
use ic_cdk::api::management_canister::http_request::{CanisterHttpRequestArgument, HttpResponse};
use ic_stable_structures::memory_manager::VirtualMemory;
use ic_stable_structures::{
    memory_manager::{MemoryId, MemoryManager},
    DefaultMemoryImpl,
};
use ic_stable_structures::{Cell, StableBTreeMap};
use std::cell::RefCell;
use tower::{BoxError, Service, ServiceBuilder};
use tower_http::classify::{NeverClassifyEos, ServerErrorsFailureClass};
use tower_http::trace::{ResponseBody, TraceLayer};
use tower_http::ServiceBuilderExt;

const IS_DEMO_ACTIVE_MEMORY_ID: MemoryId = MemoryId::new(4);
const API_KEY_MAP_MEMORY_ID: MemoryId = MemoryId::new(5);
const MANAGE_API_KEYS_MEMORY_ID: MemoryId = MemoryId::new(6);
const LOG_FILTER_MEMORY_ID: MemoryId = MemoryId::new(7);
const OVERRIDE_PROVIDER_MEMORY_ID: MemoryId = MemoryId::new(8);
const NUM_SUBNET_NODES_MEMORY_ID: MemoryId = MemoryId::new(9);

type StableMemory = VirtualMemory<DefaultMemoryImpl>;

thread_local! {
    // Unstable static data: these are reset when the canister is upgraded.
    pub static UNSTABLE_METRICS: RefCell<Metrics> = RefCell::new(Metrics::default());
    static UNSTABLE_HTTP_REQUEST_COUNTER: RefCell<u64> = const {RefCell::new(0)};

    // Stable static data: these are preserved when the canister is upgraded.
    static MEMORY_MANAGER: RefCell<MemoryManager<DefaultMemoryImpl>> =
        RefCell::new(MemoryManager::init(DefaultMemoryImpl::default()));
    static IS_DEMO_ACTIVE: RefCell<Cell<bool, StableMemory>> =
        RefCell::new(Cell::init(MEMORY_MANAGER.with_borrow(|m| m.get(IS_DEMO_ACTIVE_MEMORY_ID)), false).expect("Unable to read demo status from stable memory"));
    static API_KEY_MAP: RefCell<StableBTreeMap<ProviderId, ApiKey, StableMemory>> =
        RefCell::new(StableBTreeMap::init(MEMORY_MANAGER.with_borrow(|m| m.get(API_KEY_MAP_MEMORY_ID))));
    static MANAGE_API_KEYS: RefCell<ic_stable_structures::Vec<Principal, StableMemory>> =
        RefCell::new(ic_stable_structures::Vec::init(MEMORY_MANAGER.with_borrow(|m| m.get(MANAGE_API_KEYS_MEMORY_ID))).expect("Unable to read API key principals from stable memory"));
    static LOG_FILTER: RefCell<Cell<LogFilter, StableMemory>> =
        RefCell::new(Cell::init(MEMORY_MANAGER.with_borrow(|m| m.get(LOG_FILTER_MEMORY_ID)), LogFilter::default()).expect("Unable to read log message filter from stable memory"));
    static OVERRIDE_PROVIDER: RefCell<Cell<OverrideProvider, StableMemory>> =
        RefCell::new(Cell::init(MEMORY_MANAGER.with_borrow(|m| m.get(OVERRIDE_PROVIDER_MEMORY_ID)), OverrideProvider::default()).expect("Unable to read provider override from stable memory"));
    static NUM_SUBNET_NODES: RefCell<Cell<u32, StableMemory>> =
        RefCell::new(Cell::init(MEMORY_MANAGER.with_borrow(|m| m.get(NUM_SUBNET_NODES_MEMORY_ID)), crate::constants::NODES_IN_SUBNET).expect("Unable to read number of subnet nodes from stable memory"));
}

pub fn get_api_key(provider_id: ProviderId) -> Option<ApiKey> {
    API_KEY_MAP.with_borrow_mut(|map| map.get(&provider_id))
}

pub fn insert_api_key(provider_id: ProviderId, api_key: ApiKey) {
    API_KEY_MAP.with_borrow_mut(|map| map.insert(provider_id, api_key));
}

pub fn remove_api_key(provider_id: ProviderId) {
    API_KEY_MAP.with_borrow_mut(|map| map.remove(&provider_id));
}

pub fn is_api_key_principal(principal: &Principal) -> bool {
    MANAGE_API_KEYS.with_borrow(|principals| principals.iter().any(|other| &other == principal))
}

pub fn set_api_key_principals(new_principals: Vec<Principal>) {
    MANAGE_API_KEYS.with_borrow_mut(|principals| {
        while !principals.is_empty() {
            principals.pop();
        }
        for principal in new_principals {
            principals
                .push(&principal)
                .expect("Error while adding API key principal");
        }
    });
}

pub fn is_demo_active() -> bool {
    IS_DEMO_ACTIVE.with_borrow(|demo| *demo.get())
}

pub fn set_demo_active(is_active: bool) {
    IS_DEMO_ACTIVE.with_borrow_mut(|demo| {
        demo.set(is_active)
            .expect("Error while storing new demo status")
    });
}

pub fn get_log_filter() -> LogFilter {
    LOG_FILTER.with_borrow(|filter| filter.get().clone())
}

pub fn set_log_filter(filter: LogFilter) {
    LOG_FILTER.with_borrow_mut(|state| {
        state
            .set(filter)
            .expect("Error while updating log message filter")
    });
}

pub fn get_override_provider() -> OverrideProvider {
    OVERRIDE_PROVIDER.with_borrow(|provider| provider.get().clone())
}

pub fn set_override_provider(provider: OverrideProvider) {
    OVERRIDE_PROVIDER.with_borrow_mut(|state| {
        state
            .set(provider)
            .expect("Error while updating override provider")
    });
}

pub fn next_request_id() -> u64 {
    UNSTABLE_HTTP_REQUEST_COUNTER.with_borrow_mut(|counter| {
        let current_request_id = *counter;
        // overflow is not an issue here because we only use `next_request_id` to correlate
        // requests and responses in logs.
        *counter = counter.wrapping_add(1);
        current_request_id
    })
}

pub fn get_num_subnet_nodes() -> u32 {
    NUM_SUBNET_NODES.with_borrow(|state| *state.get())
}

pub fn set_num_subnet_nodes(nodes: u32) {
    NUM_SUBNET_NODES.with_borrow_mut(|state| {
        state
            .set(nodes)
            .expect("Error while updating number of subnet nodes")
    });
}

pub fn http_client(
) -> impl Service<CanisterHttpRequestArgument, Response = HttpResponse, Error = BoxError> {
    ServiceBuilder::new()
        .filter(CyclesAccounting::new(
            get_num_subnet_nodes(),
            ChargingPolicyWithCollateral::default(),
        ))
        .service(canhttp::Client)
}

pub fn http_client_no_retry() -> impl Service<
    canhttp::HttpRequest,
    Response = http::Response<
        ResponseBody<FullBytes, NeverClassifyEos<ServerErrorsFailureClass>, (), (), ()>,
    >,
    Error = RpcError,
> {
    ServiceBuilder::new()
        .layer(
            TraceLayer::new_for_http()
                .on_request(())
                .on_response(())
                .on_body_chunk(())
                .on_eos(())
                .on_failure(()),
        )
        .insert_request_header_if_not_present(
            CONTENT_TYPE,
            HeaderValue::from_static(CONTENT_TYPE_VALUE),
        )
        .map_err(map_error)
        .filter(HttpRequestFilter)
        .map_response(map_ic_http_response)
        .filter(CyclesAccounting::new(
            RequestCyclesCostWithCollateralEstimator::default(),
        ))
        .service(canhttp::Client)
}

fn map_error(e: BoxError) -> RpcError {
    if let Some(charging_error) = e.downcast_ref::<CyclesAccountingError>() {
        return match charging_error {
            CyclesAccountingError::InsufficientCyclesError { expected, received } => {
                ProviderError::TooFewCycles {
                    expected: *expected,
                    received: *received,
                }
                    .into()
            }
        };
    }
    if let Some(canhttp::IcError { code, message }) = e.downcast_ref::<canhttp::IcError>() {
        // add_metric_entry!(err_http_outcall, (rpc_method, rpc_host, *code), 1);
        return HttpOutcallError::IcError {
            code: *code,
            message: message.clone(),
        }
            .into();
    }
    RpcError::ProviderError(ProviderError::InvalidRpcConfig(format!(
        "Unknown error: {}",
        e
    )))
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ChargingPolicyWithCollateral {
    charge_user: bool,
    collateral_cycles: u128,
}

impl ChargingPolicyWithCollateral {
    pub fn new(
        num_nodes_in_subnet: u32,
        charge_user: bool,
        collateral_cycles_per_node: u128,
    ) -> Self {
        let collateral_cycles =
            collateral_cycles_per_node.saturating_mul(num_nodes_in_subnet as u128);
        Self {
            charge_user,
            collateral_cycles,
        }
    }
}

impl Default for ChargingPolicyWithCollateral {
    fn default() -> Self {
        Self::new(
            get_num_subnet_nodes(),
            !is_demo_active(),
            COLLATERAL_CYCLES_PER_NODE,
        )
    }
}

impl CyclesChargingPolicy for ChargingPolicyWithCollateral {
    fn cycles_to_charge(
        &self,
        _request: &CanisterHttpRequestArgument,
        attached_cycles: u128,
    ) -> u128 {
        if self.charge_user {
            return attached_cycles.saturating_add(self.collateral_cycles);
        }
        0
    }
}

#[cfg(test)]
mod test {
    use candid::Principal;

    use crate::memory::{is_api_key_principal, set_api_key_principals};

    #[test]
    fn test_api_key_principals() {
        let principal1 =
            Principal::from_text("k5dlc-ijshq-lsyre-qvvpq-2bnxr-pb26c-ag3sc-t6zo5-rdavy-recje-zqe")
                .unwrap();
        let principal2 =
            Principal::from_text("yxhtl-jlpgx-wqnzc-ysego-h6yqe-3zwfo-o3grn-gvuhm-nz3kv-ainub-6ae")
                .unwrap();
        assert!(!is_api_key_principal(&principal1));
        assert!(!is_api_key_principal(&principal2));

        set_api_key_principals(vec![principal1]);
        assert!(is_api_key_principal(&principal1));
        assert!(!is_api_key_principal(&principal2));

        set_api_key_principals(vec![principal2]);
        assert!(!is_api_key_principal(&principal1));
        assert!(is_api_key_principal(&principal2));

        set_api_key_principals(vec![principal1, principal2]);
        assert!(is_api_key_principal(&principal1));
        assert!(is_api_key_principal(&principal2));

        set_api_key_principals(vec![]);
        assert!(!is_api_key_principal(&principal1));
        assert!(!is_api_key_principal(&principal2));
    }
}

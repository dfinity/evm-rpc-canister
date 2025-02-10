use crate::constants::{COLLATERAL_CYCLES_PER_NODE, NODES_IN_SUBNET};
use crate::types::{ApiKey, LogFilter, Metrics, OverrideProvider, ProviderId};
use candid::Principal;
use canhttp::{ChargeCaller, DefaultRequestCyclesCostEstimator, EstimateRequestCyclesCost};
use ic_cdk::api::management_canister::http_request::{CanisterHttpRequestArgument, HttpResponse};
use ic_stable_structures::memory_manager::VirtualMemory;
use ic_stable_structures::{
    memory_manager::{MemoryId, MemoryManager},
    DefaultMemoryImpl,
};
use ic_stable_structures::{Cell, StableBTreeMap};
use std::cell::RefCell;
use tower::filter::FilterLayer;
use tower::{Service, ServiceBuilder};

const IS_DEMO_ACTIVE_MEMORY_ID: MemoryId = MemoryId::new(4);
const API_KEY_MAP_MEMORY_ID: MemoryId = MemoryId::new(5);
const MANAGE_API_KEYS_MEMORY_ID: MemoryId = MemoryId::new(6);
const LOG_FILTER_MEMORY_ID: MemoryId = MemoryId::new(7);
const OVERRIDE_PROVIDER_MEMORY_ID: MemoryId = MemoryId::new(8);

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

pub fn http_client() -> impl Service<CanisterHttpRequestArgument, Response = HttpResponse> {
    ServiceBuilder::new()
        .option_layer(if !is_demo_active() {
            Some(FilterLayer::new(ChargeCaller::new(
                RequestCyclesCostWithCollateralEstimator::new(NODES_IN_SUBNET),
            )))
        } else {
            None
        })
        .service(canhttp::Client::new(NODES_IN_SUBNET))
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct RequestCyclesCostWithCollateralEstimator {
    num_nodes_in_subnet: u32,
    inner: DefaultRequestCyclesCostEstimator,
}

impl RequestCyclesCostWithCollateralEstimator {
    pub fn new(num_nodes_in_subnet: u32) -> Self {
        Self {
            num_nodes_in_subnet,
            inner: DefaultRequestCyclesCostEstimator::new(num_nodes_in_subnet),
        }
    }
}

impl EstimateRequestCyclesCost for RequestCyclesCostWithCollateralEstimator {
    fn cycles_cost(&self, request: &CanisterHttpRequestArgument) -> u128 {
        self.inner.cycles_cost(request).saturating_add(
            COLLATERAL_CYCLES_PER_NODE.saturating_mul(self.num_nodes_in_subnet as u128),
        )
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

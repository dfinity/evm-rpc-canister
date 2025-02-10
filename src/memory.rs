use crate::constants::NODES_IN_SUBNET;
use crate::types::{ApiKey, LogFilter, Metrics, OverrideProvider, ProviderId};
use bytes::Bytes;
use candid::Principal;
use canjsonrpc::client::HttpOutcallError;
use canjsonrpc::cycles::{ChargeCaller, DefaultRequestCost};
use canjsonrpc::http::{convert_response, IntoCanisterHttpRequest};
use canjsonrpc::json::{JsonRpcRequest, JsonRpcResponse};
use canjsonrpc::retry::DoubleMaxResponseBytes;
use ic_cdk::api::management_canister::http_request::{CanisterHttpRequestArgument, HttpResponse};
use ic_stable_structures::memory_manager::VirtualMemory;
use ic_stable_structures::{
    memory_manager::{MemoryId, MemoryManager},
    DefaultMemoryImpl,
};
use ic_stable_structures::{Cell, StableBTreeMap};
use std::cell::RefCell;
use tower::util::BoxCloneService;
use tower::{BoxError, ServiceBuilder, ServiceExt};
use tower_http::classify::StatusInRangeAsFailures;
use tower_http::trace::TraceLayer;

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

fn canister_http_client() -> BoxCloneService<CanisterHttpRequestArgument, HttpResponse, BoxError> {
    let client = canjsonrpc::client::Client::new(NODES_IN_SUBNET);
    if is_demo_active() {
        return ServiceBuilder::new()
            .retry(DoubleMaxResponseBytes)
            .service(client)
            .map_err(|e| BoxError::from(e))
            .boxed_clone();
    }
    let request_cost_estimator = DefaultRequestCost::new(NODES_IN_SUBNET);
    ServiceBuilder::new()
        .filter(ChargeCaller::new(request_cost_estimator))
        .retry(DoubleMaxResponseBytes)
        .service(client)
        .boxed_clone()
}

pub fn canister_http() -> BoxCloneService<http::Request<Bytes>, http::Response<Bytes>, BoxError> {
    ServiceBuilder::new()
        .map_request(|r: http::Request<Bytes>| r.into_canister_http_request())
        .map_response(|response: HttpResponse| convert_response(response))
        .service(canister_http_client())
        .boxed_clone()
}

pub fn canister_json_rpc() -> BoxCloneService<
    http::Request<JsonRpcRequest<serde_json::Value>>,
    http::Response<JsonRpcResponse<serde_json::Value>>,
    BoxError,
> {
    ServiceBuilder::new()
        .map_request(
            |request: http::Request<JsonRpcRequest<serde_json::Value>>| {
                let (parts, body) = request.into_parts();
                let serialized_body = Bytes::from(serde_json::to_vec(&body).unwrap());
                http::Request::from_parts(parts, serialized_body)
            },
        )
        .map_result(|result: Result<http::Response<Bytes>, HttpOutcallError>| {
            match result {
                Ok(response) => {
                    // JSON-RPC responses over HTTP should have a 2xx status code,
                    // even if the contained JsonRpcResult is an error.
                    // If the server is not available, it will sometimes (wrongly) return HTML that will fail parsing as JSON.
                    if !response.status().is_success() {
                        return Err(BoxError::from(
                            canjsonrpc::json::JsonRpcError::InvalidHttpJsonRpcResponse {
                                status: response.status().as_u16(),
                                body: String::from_utf8_lossy(&response.into_body()).to_string(),
                                parsing_error: None,
                            },
                        ));
                    }
                    let (parts, body) = response.into_parts();
                    let deser_body =
                        serde_json::from_slice::<JsonRpcResponse<serde_json::Value>>(body.as_ref())
                            .map_err(|e| {
                                BoxError::from(
                                    canjsonrpc::json::JsonRpcError::InvalidHttpJsonRpcResponse {
                                        status: parts.status.as_u16(),
                                        body: String::from_utf8_lossy(body.as_ref()).to_string(),
                                        parsing_error: Some(e.to_string()),
                                    },
                                )
                            })?;
                    Ok(http::Response::from_parts(parts, deser_body))
                }
                Err(e) => Err(BoxError::from(e)),
            }
        })
        .service(canister_http())
        .boxed_clone()
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

use candid::{candid_method, CandidType};
use ic_canister_log::log;
use ic_canisters_http_types::{
    HttpRequest as AssetHttpRequest, HttpResponse as AssetHttpResponse, HttpResponseBuilder,
};
use ic_cdk::api::management_canister::http_request::{HttpHeader, HttpResponse, TransformArgs};
use ic_cdk_macros::{query, update};
use ic_nervous_system_common::{serve_logs, serve_logs_v2, serve_metrics};

use ic_eth_rpc::*;

#[update]
#[candid_method]
async fn request(
    service_url: String,
    json_rpc_payload: String,
    max_response_bytes: u64,
) -> Result<Vec<u8>, EthRpcError> {
    do_http_request(
        ResolvedSource::Url(service_url),
        &json_rpc_payload,
        max_response_bytes,
    )
    .await
}

#[update]
#[candid_method]
async fn provider_request(
    provider_id: u64,
    json_rpc_payload: String,
    max_response_bytes: u64,
) -> Result<Vec<u8>, EthRpcError> {
    let provider = PROVIDERS.with(|p| {
        p.borrow()
            .get(&provider_id)
            .ok_or(EthRpcError::ProviderNotFound)
    })?;
    do_http_request(
        ResolvedSource::Provider(provider),
        &json_rpc_payload,
        max_response_bytes,
    )
    .await
}

#[query]
#[candid_method(query)]
fn request_cost(service_url: String, json_rpc_payload: String, max_response_bytes: u64) -> u128 {
    get_request_cost(&json_rpc_payload, &service_url, max_response_bytes)
}

#[query]
#[candid_method(query)]
fn provider_request_cost(
    provider_id: u64,
    json_rpc_payload: String,
    max_response_bytes: u64,
) -> Option<u128> {
    let provider = PROVIDERS.with(|p| p.borrow().get(&provider_id))?;
    let request_cost = get_request_cost(
        &json_rpc_payload,
        &provider.service_url(),
        max_response_bytes,
    );
    let provider_cost = get_provider_cost(&json_rpc_payload, &provider);
    Some(request_cost + provider_cost)
}

#[query]
#[candid_method(query)]
fn get_providers() -> Vec<RegisteredProvider> {
    PROVIDERS.with(|p| {
        p.borrow()
            .iter()
            .map(|(_, e)| RegisteredProvider {
                provider_id: e.provider_id,
                owner: e.owner,
                chain_id: e.chain_id,
                base_url: e.base_url,
                cycles_per_call: e.cycles_per_call,
                cycles_per_message_byte: e.cycles_per_message_byte,
            })
            .collect::<Vec<RegisteredProvider>>()
    })
}

#[update(guard = "require_register_provider")]
#[candid_method]
fn register_provider(provider: RegisterProvider) -> u64 {
    let parsed_url = url::Url::parse(&provider.base_url).expect("unable to parse service_url");
    let host = parsed_url.host_str().expect("service_url host missing");
    validate_base_url(host);
    validate_credential_path(&provider.credential_path);
    let provider_id = METADATA.with(|m| {
        let mut metadata = m.borrow().get().clone();
        metadata.next_provider_id += 1;
        m.borrow_mut().set(metadata.clone()).unwrap();
        metadata.next_provider_id - 1
    });
    PROVIDERS.with(|p| {
        p.borrow_mut().insert(
            provider_id,
            Provider {
                provider_id,
                owner: ic_cdk::caller(),
                chain_id: provider.chain_id,
                base_url: provider.base_url,
                credential_path: provider.credential_path,
                cycles_per_call: provider.cycles_per_call,
                cycles_per_message_byte: provider.cycles_per_message_byte,
                cycles_owed: 0,
            },
        )
    });
    provider_id
}

#[update(guard = "require_register_provider")]
#[candid_method]
fn update_provider_credential(provider_id: u64, credential_path: String) {
    validate_credential_path(&credential_path);
    PROVIDERS.with(|p| match p.borrow_mut().get(&provider_id) {
        Some(mut provider) => {
            if provider.owner != ic_cdk::caller() && !is_authorized(Auth::Admin) {
                ic_cdk::trap("Provider owner != caller");
            }
            provider.credential_path = credential_path;
            p.borrow_mut().insert(provider_id, provider);
        }
        None => ic_cdk::trap("Provider not found"),
    });
}

#[update(guard = "require_register_provider")]
#[candid_method]
fn unregister_provider(provider_id: u64) {
    PROVIDERS.with(|p| {
        if let Some(provider) = p.borrow().get(&provider_id) {
            if provider.owner == ic_cdk::caller() || is_authorized(Auth::Admin) {
                p.borrow_mut().remove(&provider_id);
            } else {
                ic_cdk::trap("Not authorized");
            }
        }
    });
}

#[query(guard = "require_register_provider")]
#[candid_method(query)]
fn get_owed_cycles(provider_id: u64) -> u128 {
    let provider = PROVIDERS.with(|p| {
        p.borrow()
            .get(&provider_id)
            .ok_or(EthRpcError::ProviderNotFound)
    });
    let provider = provider.expect("Provider not found");
    if ic_cdk::caller() != provider.owner {
        ic_cdk::trap("Not owner");
    }
    provider.cycles_owed
}

#[derive(CandidType)]
struct DepositCyclesArgs {
    canister_id: Principal,
}

#[update(guard = "require_register_provider")]
#[candid_method]
async fn withdraw_owed_cycles(provider_id: u64, canister_id: Principal) {
    let provider = PROVIDERS.with(|p| {
        p.borrow()
            .get(&provider_id)
            .ok_or(EthRpcError::ProviderNotFound)
    });
    let mut provider = provider.expect("Provider not found");
    if ic_cdk::caller() != provider.owner {
        ic_cdk::trap("Not owner");
    }
    let amount = provider.cycles_owed;
    if amount < MINIMUM_WITHDRAWAL_CYCLES {
        ic_cdk::trap("Too few cycles to withdraw");
    }
    PROVIDERS.with(|p| {
        provider.cycles_owed = 0;
        p.borrow_mut().insert(provider_id, provider)
    });
    match ic_cdk::api::call::call_with_payment128(
        Principal::management_canister(),
        "deposit_cycles",
        (DepositCyclesArgs { canister_id },),
        amount,
    )
    .await
    {
        Ok(()) => (),
        e => {
            // Refund on failure to send cycles.
            log!(
                INFO,
                "Unable to send {} cycles to {} for provider {}: {:?}",
                amount,
                canister_id,
                provider_id,
                e
            );
            let provider = PROVIDERS.with(|p| {
                p.borrow()
                    .get(&provider_id)
                    .ok_or(EthRpcError::ProviderNotFound)
            });
            let mut provider = provider.expect("Provider not found during refund, cycles lost.");
            PROVIDERS.with(|p| {
                provider.cycles_owed += amount;
                p.borrow_mut().insert(provider_id, provider)
            });
        }
    };
}

#[query(name = "__transform_json_rpc")]
fn transform(args: TransformArgs) -> HttpResponse {
    HttpResponse {
        status: args.response.status.clone(),
        body: args.response.body,
        // Strip headers as they contain the Date which is not necessarily the same
        // and will prevent consensus on the result.
        headers: Vec::<HttpHeader>::new(),
    }
}

#[ic_cdk::init]
fn init() {
    initialize();
    METADATA.with(|m| {
        let mut metadata = m.borrow().get().clone();
        metadata.nodes_in_subnet = DEFAULT_NODES_IN_SUBNET;
        metadata.open_rpc_access = DEFAULT_OPEN_RPC_ACCESS;
        m.borrow_mut().set(metadata).unwrap();
    });
}

#[ic_cdk::post_upgrade]
fn post_upgrade() {
    initialize();
    stable_authorize(ic_cdk::caller());
}

#[query]
fn http_request(request: AssetHttpRequest) -> AssetHttpResponse {
    match request.path() {
        "/metrics" => serve_metrics(encode_metrics),
        "/logs" => serve_logs_v2(request, &INFO, &ERROR),
        "/log/info" => serve_logs(&INFO),
        "/log/error" => serve_logs(&ERROR),
        _ => HttpResponseBuilder::not_found().build(),
    }
}

#[update(guard = "require_stable_authorized")]
fn stable_authorize(principal: Principal) {
    AUTH_STABLE.with(|a| a.borrow_mut().insert(principal));
}

#[query(guard = "require_stable_authorized")]
fn stable_size() -> u64 {
    ic_cdk::api::stable::stable64_size() * WASM_PAGE_SIZE
}

#[query(guard = "require_stable_authorized")]
fn stable_read(offset: u64, length: u64) -> Vec<u8> {
    let mut buffer = Vec::new();
    buffer.resize(length as usize, 0);
    ic_cdk::api::stable::stable64_read(offset, buffer.as_mut_slice());
    buffer
}

#[update(guard = "require_stable_authorized")]
fn stable_write(offset: u64, buffer: Vec<u8>) {
    let size = offset + buffer.len() as u64;
    let old_size = ic_cdk::api::stable::stable64_size() * WASM_PAGE_SIZE;
    if size > old_size {
        let old_pages = old_size / WASM_PAGE_SIZE;
        let pages = (size + (WASM_PAGE_SIZE - 1)) / WASM_PAGE_SIZE;
        ic_cdk::api::stable::stable64_grow(pages - old_pages).unwrap();
    }
    ic_cdk::api::stable::stable64_write(offset, buffer.as_slice());
}

#[update(guard = "require_admin_or_controller")]
#[candid_method]
fn authorize(principal: Principal, auth: Auth) {
    do_authorize(principal, auth)
}

#[query(guard = "require_admin_or_controller")]
#[candid_method(query)]
fn get_authorized(auth: Auth) -> Vec<String> {
    AUTH.with(|a| {
        let mut result = Vec::new();
        for (k, v) in a.borrow().iter() {
            if v & (auth.clone() as u32) != 0 {
                result.push(k.0.to_string());
            }
        }
        result
    })
}

#[update(guard = "require_admin_or_controller")]
#[candid_method]
fn deauthorize(principal: Principal, auth: Auth) {
    do_deauthorize(principal, auth)
}

#[update(guard = "require_admin_or_controller")]
#[candid_method]
fn set_open_rpc_access(open_rpc_access: bool) {
    METADATA.with(|m| {
        let mut metadata = m.borrow().get().clone();
        metadata.open_rpc_access = open_rpc_access;
        m.borrow_mut().set(metadata).unwrap();
    });
}

#[query(guard = "require_admin_or_controller")]
#[candid_method(query)]
fn get_open_rpc_access() -> bool {
    METADATA.with(|m| m.borrow().get().open_rpc_access)
}

#[update(guard = "require_admin_or_controller")]
#[candid_method]
fn set_nodes_in_subnet(nodes_in_subnet: u32) {
    METADATA.with(|m| {
        let mut metadata = m.borrow().get().clone();
        metadata.nodes_in_subnet = nodes_in_subnet;
        m.borrow_mut().set(metadata).unwrap();
    });
}

#[query(guard = "require_admin_or_controller")]
#[candid_method(query)]
fn get_nodes_in_subnet() -> u32 {
    METADATA.with(|m| m.borrow().get().nodes_in_subnet)
}

fn initialize() {
    SERVICE_HOSTS_ALLOWLIST
        .with(|a| (*a.borrow_mut()) = AllowlistSet::from_iter(INITIAL_SERVICE_HOSTS_ALLOWLIST));

    for principal in RPC_ALLOWLIST.iter() {
        authorize(to_principal(principal), Auth::Rpc);
    }
    for principal in REGISTER_PROVIDER_ALLOWLIST.iter() {
        authorize(to_principal(principal), Auth::RegisterProvider);
    }
    for principal in FREE_RPC_ALLOWLIST.iter() {
        authorize(to_principal(principal), Auth::FreeRpc);
    }
    for principal in AUTHORIZED_ADMIN.iter() {
        authorize(to_principal(principal), Auth::Admin);
    }
}

#[cfg(not(any(target_arch = "wasm32", test)))]
fn main() {
    candid::export_service!();
    std::print!("{}", __export_service());
}

#[cfg(any(target_arch = "wasm32", test))]
fn main() {}

#[test]
fn test_candid_interface() {
    use candid::utils::{service_compatible, CandidSource};
    use std::path::Path;

    candid::export_service!();
    let new_interface = __export_service();

    service_compatible(
        CandidSource::Text(&new_interface),
        CandidSource::File(Path::new("candid/ic_eth.did")),
    )
    .unwrap();
}

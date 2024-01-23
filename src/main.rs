use candid::{candid_method, CandidType};
use cketh_common::eth_rpc::{
    Block, FeeHistory, LogEntry, ProviderError, RpcError, SendRawTransactionResult,
};

use cketh_common::eth_rpc_client::providers::RpcService;
use cketh_common::logs::INFO;
use ic_canister_log::log;
use ic_canisters_http_types::{
    HttpRequest as AssetHttpRequest, HttpResponse as AssetHttpResponse, HttpResponseBuilder,
};
use ic_cdk::api::management_canister::http_request::{HttpResponse, TransformArgs};
use ic_cdk::{query, update};
use ic_nervous_system_common::serve_metrics;

use evm_rpc::*;

#[update(name = "eth_getLogs")]
#[candid_method(rename = "eth_getLogs")]
pub async fn eth_get_logs(
    source: RpcSource,
    args: candid_types::GetLogsArgs,
) -> MultiRpcResult<Vec<LogEntry>> {
    match CandidRpcClient::from_source(source) {
        Ok(source) => source.eth_get_logs(args).await,
        Err(err) => Err(err).into(),
    }
}

#[update(name = "eth_getBlockByNumber")]
#[candid_method(rename = "eth_getBlockByNumber")]
pub async fn eth_get_block_by_number(
    source: RpcSource,
    block: candid_types::BlockTag,
) -> MultiRpcResult<Block> {
    match CandidRpcClient::from_source(source) {
        Ok(source) => source.eth_get_block_by_number(block).await,
        Err(err) => Err(err).into(),
    }
}

#[update(name = "eth_getTransactionReceipt")]
#[candid_method(rename = "eth_getTransactionReceipt")]
pub async fn eth_get_transaction_receipt(
    source: RpcSource,
    hash: String,
) -> MultiRpcResult<Option<candid_types::TransactionReceipt>> {
    match CandidRpcClient::from_source(source) {
        Ok(source) => source.eth_get_transaction_receipt(hash).await,
        Err(err) => Err(err).into(),
    }
}

#[update(name = "eth_getTransactionCount")]
#[candid_method(rename = "eth_getTransactionCount")]
pub async fn eth_get_transaction_count(
    source: RpcSource,
    args: candid_types::GetTransactionCountArgs,
) -> MultiRpcResult<candid::Nat> {
    match CandidRpcClient::from_source(source) {
        Ok(source) => source.eth_get_transaction_count(args).await,
        Err(err) => Err(err).into(),
    }
}

#[update(name = "eth_feeHistory")]
#[candid_method(rename = "eth_feeHistory")]
pub async fn eth_fee_history(
    source: RpcSource,
    args: candid_types::FeeHistoryArgs,
) -> MultiRpcResult<Option<FeeHistory>> {
    match CandidRpcClient::from_source(source) {
        Ok(source) => source.eth_fee_history(args).await,
        Err(err) => Err(err).into(),
    }
}

#[update(name = "eth_sendRawTransaction")]
#[candid_method(rename = "eth_sendRawTransaction")]
pub async fn eth_send_raw_transaction(
    source: RpcSource,
    raw_signed_transaction_hex: String,
) -> MultiRpcResult<SendRawTransactionResult> {
    match CandidRpcClient::from_source(source) {
        Ok(source) => {
            source
                .eth_send_raw_transaction(raw_signed_transaction_hex)
                .await
        }
        Err(err) => Err(err).into(),
    }
}

#[update]
#[candid_method]
async fn request(
    source: JsonRpcSource,
    json_rpc_payload: String,
    max_response_bytes: u64,
) -> Result<String, RpcError> {
    let response = do_json_rpc_request(
        ic_cdk::caller(),
        source.resolve()?,
        MetricRpcMethod("request".to_string()),
        &json_rpc_payload,
        max_response_bytes,
    )
    .await?;
    get_http_response_body(response)
}

#[query(name = "requestCost")]
#[candid_method(query, rename = "requestCost")]
fn request_cost(
    source: JsonRpcSource,
    json_rpc_payload: String,
    max_response_bytes: u64,
) -> Result<u128, RpcError> {
    Ok(get_json_rpc_cost(
        &source.resolve().unwrap(),
        json_rpc_payload.len() as u64,
        max_response_bytes,
    ))
}

#[query(name = "getProviders")]
#[candid_method(query, rename = "getProviders")]
fn get_providers() -> Vec<ProviderView> {
    PROVIDERS.with(|p| {
        p.borrow()
            .iter()
            .map(|(_, provider)| provider.into())
            .collect::<Vec<ProviderView>>()
    })
}

#[update(name = "registerProvider", guard = "require_register_provider")]
#[candid_method(rename = "registerProvider")]
fn register_provider(provider: RegisterProviderArgs) -> u64 {
    do_register_provider(ic_cdk::caller(), provider)
}

#[update(name = "unregisterProvider")]
#[candid_method(rename = "unregisterProvider")]
fn unregister_provider(provider_id: u64) -> bool {
    do_unregister_provider(ic_cdk::caller(), provider_id)
}

#[update(name = "updateProvider")]
#[candid_method(rename = "updateProvider")]
fn update_provider(provider: UpdateProviderArgs) {
    do_update_provider(ic_cdk::caller(), provider)
}

#[update(name = "manageProvider", guard = "require_admin_or_controller")]
#[candid_method(rename = "manageProvider")]
fn manage_provider(args: ManageProviderArgs) {
    log!(
        INFO,
        "[{}] Managing provider: {}",
        ic_cdk::caller(),
        args.provider_id
    );
    do_manage_provider(args)
}

#[query(name = "getServiceProviderMap", guard = "require_admin_or_controller")]
#[candid_method(query, rename = "getServiceProviderMap")]
fn get_service_provider_map() -> Vec<(RpcService, u64)> {
    SERVICE_PROVIDER_MAP.with(|map| {
        map.borrow()
            .iter()
            .filter_map(|(k, v)| Some((k.try_into().ok()?, v)))
            .collect()
    })
}

#[query(name = "getAccumulatedCycleCount", guard = "require_register_provider")]
#[candid_method(query, rename = "getAccumulatedCycleCount")]
fn get_accumulated_cycle_count(provider_id: u64) -> u128 {
    let provider = PROVIDERS.with(|p| {
        p.borrow()
            .get(&provider_id)
            .ok_or(ProviderError::ProviderNotFound)
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

#[update(
    name = "withdrawAccumulatedCycles",
    guard = "require_register_provider"
)]
#[candid_method(rename = "withdrawAccumulatedCycles")]
async fn withdraw_accumulated_cycles(provider_id: u64, canister_id: Principal) {
    let provider = PROVIDERS.with(|p| {
        p.borrow()
            .get(&provider_id)
            .ok_or(ProviderError::ProviderNotFound)
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
    log!(
        INFO,
        "[{}] Withdrawing {} cycles from provider {} to canister: {}",
        ic_cdk::caller(),
        amount,
        provider_id,
        canister_id,
    );
    match ic_cdk::api::call::call_with_payment128(
        Principal::management_canister(),
        "deposit_cycles",
        (DepositCyclesArgs { canister_id },),
        amount,
    )
    .await
    {
        Ok(()) => add_metric!(cycles_withdrawn, amount),
        Err(err) => {
            // Refund on failure to send cycles.
            log!(
                INFO,
                "[{}] Unable to send {} cycles from provider {}: {:?}",
                canister_id,
                amount,
                provider_id,
                err
            );
            let provider = PROVIDERS.with(|p| {
                p.borrow()
                    .get(&provider_id)
                    .ok_or(ProviderError::ProviderNotFound)
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
    do_transform_http_request(args)
}

#[ic_cdk::init]
fn init(args: InitArgs) {
    UNSTABLE_SUBNET_SIZE.with(|m| *m.borrow_mut() = args.nodes_in_subnet);

    for provider in get_default_providers() {
        do_register_provider(ic_cdk::caller(), provider);
    }
    for (service, hostname) in get_default_service_provider_hostnames() {
        let provider =
            find_provider(|p| p.chain_id == get_chain_id(&service) && p.hostname == hostname)
                .unwrap_or_else(|| {
                    panic!(
                        "Missing default provider for service {:?} with hostname {:?}",
                        service, hostname
                    )
                });
        set_service_provider(&service, &provider);
    }
}

#[query]
fn http_request(request: AssetHttpRequest) -> AssetHttpResponse {
    match request.path() {
        "/metrics" => serve_metrics(encode_metrics),
        "/logs" => {
            use cketh_common::logs::{Log, Priority, Sort};
            use std::str::FromStr;

            let max_skip_timestamp = match request.raw_query_param("time") {
                Some(arg) => match u64::from_str(arg) {
                    Ok(value) => value,
                    Err(_) => {
                        return HttpResponseBuilder::bad_request()
                            .with_body_and_content_length("failed to parse the 'time' parameter")
                            .build()
                    }
                },
                None => 0,
            };

            let mut log: Log = Default::default();

            match request.raw_query_param("priority") {
                Some(priority_str) => match Priority::from_str(priority_str) {
                    Ok(priority) => match priority {
                        Priority::Info => log.push_logs(Priority::Info),
                        Priority::TraceHttp => log.push_logs(Priority::TraceHttp),
                        Priority::Debug => log.push_logs(Priority::Debug),
                    },
                    Err(_) => log.push_all(),
                },
                None => log.push_all(),
            }

            log.entries
                .retain(|entry| entry.timestamp >= max_skip_timestamp);

            fn ordering_from_query_params(sort: Option<&str>, max_skip_timestamp: u64) -> Sort {
                match sort {
                    Some(ord_str) => match Sort::from_str(ord_str) {
                        Ok(order) => order,
                        Err(_) => {
                            if max_skip_timestamp == 0 {
                                Sort::Ascending
                            } else {
                                Sort::Descending
                            }
                        }
                    },
                    None => {
                        if max_skip_timestamp == 0 {
                            Sort::Ascending
                        } else {
                            Sort::Descending
                        }
                    }
                }
            }

            log.sort_logs(ordering_from_query_params(
                request.raw_query_param("sort"),
                max_skip_timestamp,
            ));

            const MAX_BODY_SIZE: usize = 3_000_000;
            HttpResponseBuilder::ok()
                .header("Content-Type", "application/json; charset=utf-8")
                .with_body_and_content_length(log.serialize_logs(MAX_BODY_SIZE))
                .build()
        }
        _ => HttpResponseBuilder::not_found().build(),
    }
}

#[query(name = "getMetrics")]
#[candid_method(query, rename = "getMetrics")]
fn get_metrics() -> Metrics {
    UNSTABLE_METRICS.with(|metrics| (*metrics.borrow()).clone())
}

#[query(guard = "require_admin_or_controller")]
fn stable_size() -> u64 {
    ic_cdk::api::stable::stable64_size() * WASM_PAGE_SIZE
}

#[query(guard = "require_admin_or_controller")]
fn stable_read(offset: u64, length: u64) -> Vec<u8> {
    let mut buffer = Vec::new();
    buffer.resize(length as usize, 0);
    ic_cdk::api::stable::stable64_read(offset, buffer.as_mut_slice());
    buffer
}

#[update(guard = "require_admin_or_controller")]
#[candid_method]
fn authorize(principal: Principal, auth: Auth) {
    log!(
        INFO,
        "[{}] Authorizing `{:?}` for principal: {}",
        ic_cdk::caller(),
        auth,
        principal
    );
    do_authorize(principal, auth)
}

#[query(name = "getAuthorized", guard = "require_admin_or_controller")]
#[candid_method(query, rename = "getAuthorized")]
fn get_authorized(auth: Auth) -> Vec<Principal> {
    AUTH.with(|a| {
        let mut result = Vec::new();
        for (k, v) in a.borrow().iter() {
            if v.is_authorized(auth) {
                result.push(k.0);
            }
        }
        result
    })
}

#[update(guard = "require_admin_or_controller")]
#[candid_method]
fn deauthorize(principal: Principal, auth: Auth) {
    log!(
        INFO,
        "[{}] Deauthorizing `{:?}` for principal: {}",
        ic_cdk::caller(),
        auth,
        principal
    );
    do_deauthorize(principal, auth)
}

#[query(name = "getOpenRpcAccess", guard = "require_admin_or_controller")]
#[candid_method(query, rename = "getOpenRpcAccess")]
fn get_open_rpc_access() -> bool {
    METADATA.with(|m| m.borrow().get().open_rpc_access)
}

#[update(name = "setOpenRpcAccess", guard = "require_admin_or_controller")]
#[candid_method(rename = "setOpenRpcAccess")]
fn set_open_rpc_access(open_rpc_access: bool) {
    log!(
        INFO,
        "[{}] Setting open RPC access to `{}`",
        ic_cdk::caller(),
        open_rpc_access
    );
    METADATA.with(|m| {
        let mut metadata = m.borrow().get().clone();
        metadata.open_rpc_access = open_rpc_access;
        m.borrow_mut().set(metadata).unwrap();
    });
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
    fn source_to_str(source: &candid::utils::CandidSource) -> String {
        match source {
            candid::utils::CandidSource::File(f) => {
                std::fs::read_to_string(f).unwrap_or_else(|_| "".to_string())
            }
            candid::utils::CandidSource::Text(t) => t.to_string(),
        }
    }

    fn check_service_compatible(
        new_name: &str,
        new: candid::utils::CandidSource,
        old_name: &str,
        old: candid::utils::CandidSource,
    ) {
        let new_str = source_to_str(&new);
        let old_str = source_to_str(&old);
        match candid::utils::service_compatible(new, old) {
            Ok(_) => {}
            Err(e) => {
                eprintln!(
                    "{} is not compatible with {}!\n\n\
            {}:\n\
            {}\n\n\
            {}:\n\
            {}\n",
                    new_name, old_name, new_name, new_str, old_name, old_str
                );
                panic!("{:?}", e);
            }
        }
    }

    candid::export_service!();
    let new_interface = __export_service();

    // check the public interface against the actual one
    let old_interface = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("candid/evm_rpc.did");

    check_service_compatible(
        "actual ledger candid interface",
        candid::utils::CandidSource::Text(&new_interface),
        "declared candid interface in evm_rpc.did file",
        candid::utils::CandidSource::File(old_interface.as_path()),
    );
}

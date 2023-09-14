use ic_canister_log::log;
use ic_cdk::api::management_canister::http_request::{
    http_request as make_http_request, CanisterHttpRequestArgument, HttpHeader, HttpMethod,
    TransformContext,
};

use crate::*;

pub async fn do_http_request(
    source: ResolvedSource,
    json_rpc_payload: &str,
    max_response_bytes: u64,
) -> Result<Vec<u8>, EthRpcError> {
    inc_metric!(requests);
    if !is_authorized(Auth::Rpc) {
        inc_metric!(request_err_no_permission);
        return Err(EthRpcError::NoPermission);
    }
    let cycles_available = ic_cdk::api::call::msg_cycles_available128();
    let (service_url, provider) = match source {
        ResolvedSource::Url(url) => (url, None),
        ResolvedSource::Provider(provider) => (provider.service_url(), Some(provider)),
    };
    let parsed_url = url::Url::parse(&service_url).or(Err(EthRpcError::ServiceUrlParseError))?;
    let host = parsed_url
        .host_str()
        .ok_or(EthRpcError::ServiceUrlHostMissing)?
        .to_string();
    if SERVICE_HOSTS_ALLOWLIST.with(|a| !a.borrow().contains(&host.as_str())) {
        log!(INFO, "host not allowed: {}", host);
        inc_metric!(request_err_host_not_allowed);
        return Err(EthRpcError::ServiceUrlHostNotAllowed);
    }
    let request_cost = get_request_cost(json_rpc_payload, &service_url, max_response_bytes);
    let provider_cost = provider
        .as_ref()
        .map_or(0, |provider| get_provider_cost(json_rpc_payload, provider));
    let cost = request_cost + provider_cost;
    if !is_authorized(Auth::FreeRpc) {
        if cycles_available < cost {
            return Err(EthRpcError::TooFewCycles(format!(
                "requires {cost} cycles, got {cycles_available} cycles",
            )));
        }
        ic_cdk::api::call::msg_cycles_accept128(cost);
        if let Some(mut provider) = provider {
            provider.cycles_owed += provider_cost;
            PROVIDERS.with(|p| {
                // Error should not happen here as it was checked before.
                p.borrow_mut()
                    .insert(provider.provider_id, provider)
                    .expect("unable to update Provider");
            });
        }
        add_metric!(request_cycles_charged, cost);
        add_metric!(request_cycles_refunded, cycles_available - cost);
    }
    inc_metric_entry!(host_requests, host);
    let request_headers = vec![
        HttpHeader {
            name: "Content-Type".to_string(),
            value: "application/json".to_string(),
        },
        HttpHeader {
            name: "Host".to_string(),
            value: host.to_string(),
        },
    ];
    let request = CanisterHttpRequestArgument {
        url: service_url,
        max_response_bytes: Some(max_response_bytes),
        method: HttpMethod::POST,
        headers: request_headers,
        body: Some(json_rpc_payload.as_bytes().to_vec()),
        transform: Some(TransformContext::from_name(
            "__transform_json_rpc".to_string(),
            vec![],
        )),
    };
    match make_http_request(request, cost).await {
        Ok((result,)) => Ok(result.body),
        Err((r, m)) => {
            inc_metric!(request_err_http);
            Err(EthRpcError::HttpRequestError {
                code: r as u32,
                message: m,
            })
        }
    }
}

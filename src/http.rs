use crate::{
    accounting::{get_cost_with_collateral, get_http_request_cost},
    add_metric_entry,
    constants::{CONTENT_TYPE_HEADER_LOWERCASE, CONTENT_TYPE_VALUE, DEFAULT_MAX_RESPONSE_BYTES},
    memory::{get_num_subnet_nodes, get_override_provider, is_demo_active},
    types::{MetricRpcHost, MetricRpcMethod, ResolvedRpcService},
    util::canonicalize_json,
};
use evm_rpc_types::{HttpOutcallError, ProviderError, RpcError, RpcResult, ValidationError};
use ic_cdk::api::management_canister::http_request::{
    CanisterHttpRequestArgument, HttpHeader, HttpMethod, HttpResponse, TransformArgs,
    TransformContext,
};
use num_traits::ToPrimitive;

pub fn json_rpc_request_arg(
    service: ResolvedRpcService,
    json_rpc_payload: &str,
    max_response_bytes: u64,
) -> RpcResult<CanisterHttpRequestArgument> {
    let api = service.api(&get_override_provider())?;
    let mut request_headers = api.headers.unwrap_or_default();
    if !request_headers
        .iter()
        .any(|header| header.name.to_lowercase() == CONTENT_TYPE_HEADER_LOWERCASE)
    {
        request_headers.push(HttpHeader {
            name: CONTENT_TYPE_HEADER_LOWERCASE.to_string(),
            value: CONTENT_TYPE_VALUE.to_string(),
        });
    }
    Ok(CanisterHttpRequestArgument {
        url: api.url,
        max_response_bytes: Some(max_response_bytes),
        method: HttpMethod::POST,
        headers: request_headers,
        body: Some(json_rpc_payload.as_bytes().to_vec()),
        transform: Some(TransformContext::from_name(
            "__transform_json_rpc".to_string(),
            vec![],
        )),
    })
}

pub async fn json_rpc_request(
    service: ResolvedRpcService,
    rpc_method: MetricRpcMethod,
    json_rpc_payload: &str,
    max_response_bytes: u64,
) -> RpcResult<HttpResponse> {
    let request = json_rpc_request_arg(service, json_rpc_payload, max_response_bytes)?;
    http_request(rpc_method, request).await
}

pub async fn http_request(
    rpc_method: MetricRpcMethod,
    request: CanisterHttpRequestArgument,
) -> RpcResult<HttpResponse> {
    let cycles_cost = get_http_request_arg_cost(&request);
    let url = request.url.clone();
    let parsed_url = match url::Url::parse(&url) {
        Ok(url) => url,
        Err(_) => {
            return Err(ValidationError::Custom(format!("Error parsing URL: {}", url)).into())
        }
    };
    let host = match parsed_url.host_str() {
        Some(host) => host,
        None => {
            return Err(ValidationError::Custom(format!(
                "Error parsing hostname from URL: {}",
                url
            ))
            .into())
        }
    };
    let rpc_host = MetricRpcHost(host.to_string());
    if !is_demo_active() {
        let cycles_available = ic_cdk::api::call::msg_cycles_available128();
        let cycles_cost_with_collateral =
            get_cost_with_collateral(get_num_subnet_nodes(), cycles_cost);
        if cycles_available < cycles_cost_with_collateral {
            return Err(ProviderError::TooFewCycles {
                expected: cycles_cost_with_collateral,
                received: cycles_available,
            }
            .into());
        }
        ic_cdk::api::call::msg_cycles_accept128(cycles_cost);
        add_metric_entry!(
            cycles_charged,
            (rpc_method.clone(), rpc_host.clone()),
            cycles_cost
        );
    }
    add_metric_entry!(requests, (rpc_method.clone(), rpc_host.clone()), 1);
    match ic_cdk::api::management_canister::http_request::http_request(request, cycles_cost).await {
        Ok((response,)) => {
            let status: u32 = response.status.0.clone().try_into().unwrap_or(0);
            add_metric_entry!(responses, (rpc_method, rpc_host, status.into()), 1);
            Ok(response)
        }
        Err((code, message)) => {
            add_metric_entry!(err_http_outcall, (rpc_method, rpc_host, code), 1);
            Err(HttpOutcallError::IcError { code, message }.into())
        }
    }
}

pub fn transform_http_request(args: TransformArgs) -> HttpResponse {
    HttpResponse {
        status: args.response.status,
        body: canonicalize_json(&args.response.body).unwrap_or(args.response.body),
        // Remove headers (which may contain a timestamp) for consensus
        headers: vec![],
    }
}

pub fn get_http_response_status(status: candid::Nat) -> u16 {
    status.0.to_u16().unwrap_or(u16::MAX)
}

pub fn get_http_response_body(response: HttpResponse) -> Result<String, RpcError> {
    String::from_utf8(response.body).map_err(|e| {
        HttpOutcallError::InvalidHttpJsonRpcResponse {
            status: get_http_response_status(response.status),
            body: "".to_string(),
            parsing_error: Some(format!("{e}")),
        }
        .into()
    })
}

pub fn get_http_request_arg_cost(arg: &CanisterHttpRequestArgument) -> u128 {
    let payload_body_bytes = arg.body.as_ref().map(|body| body.len()).unwrap_or_default();
    let extra_payload_bytes = arg.url.len()
        + arg
            .headers
            .iter()
            .map(|header| header.name.len() + header.value.len())
            .sum::<usize>()
        + arg.transform.as_ref().map_or(0, |transform| {
            transform.function.0.method.len() + transform.context.len()
        });
    let max_response_bytes = arg.max_response_bytes.unwrap_or(DEFAULT_MAX_RESPONSE_BYTES);

    get_http_request_cost(
        get_num_subnet_nodes(),
        payload_body_bytes as u64,
        extra_payload_bytes as u64,
        max_response_bytes,
    )
}

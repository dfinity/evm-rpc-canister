use crate::memory::http_client;
use crate::{
    add_metric_entry,
    constants::{CONTENT_TYPE_HEADER_LOWERCASE, CONTENT_TYPE_VALUE},
    memory::get_override_provider,
    types::{MetricRpcHost, MetricRpcMethod, ResolvedRpcService},
    util::canonicalize_json,
};
use canhttp::ChargeCallerError;
use evm_rpc_types::{HttpOutcallError, ProviderError, RpcError, RpcResult, ValidationError};
use ic_cdk::api::management_canister::http_request::{
    CanisterHttpRequestArgument, HttpHeader, HttpMethod, HttpResponse, TransformArgs,
    TransformContext,
};
use num_traits::ToPrimitive;
use tower::Service;

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
    add_metric_entry!(requests, (rpc_method.clone(), rpc_host.clone()), 1);
    match http_client().call(request).await {
        Ok(response) => {
            let status: u32 = response.status.0.clone().try_into().unwrap_or(0);
            add_metric_entry!(responses, (rpc_method, rpc_host, status.into()), 1);
            Ok(response)
        }
        Err(e) => {
            if let Some(charging_error) = e.downcast_ref::<ChargeCallerError>() {
                return match charging_error {
                    ChargeCallerError::InsufficientCyclesError { expected, received } => {
                        Err(ProviderError::TooFewCycles {
                            expected: *expected,
                            received: *received,
                        }
                        .into())
                    }
                };
            }
            if let Some(canhttp::IcError { code, message }) = e.downcast_ref::<canhttp::IcError>() {
                add_metric_entry!(err_http_outcall, (rpc_method, rpc_host, *code), 1);
                return Err(HttpOutcallError::IcError {
                    code: *code,
                    message: message.clone(),
                }
                .into());
            }
            Err(RpcError::ProviderError(ProviderError::InvalidRpcConfig(
                format!("Unknown error: {}", e),
            )))
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

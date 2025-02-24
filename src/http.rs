use crate::constants::COLLATERAL_CYCLES_PER_NODE;
use crate::memory::{get_num_subnet_nodes, is_demo_active};
use crate::{
    add_metric_entry,
    constants::{CONTENT_TYPE_HEADER_LOWERCASE, CONTENT_TYPE_VALUE},
    memory::get_override_provider,
    types::{MetricRpcHost, MetricRpcMethod, ResolvedRpcService},
    util::canonicalize_json,
};
use canhttp::{
    observability::ObservabilityLayer, CyclesAccounting, CyclesAccountingError,
    CyclesChargingPolicy,
};
use evm_rpc_types::{HttpOutcallError, ProviderError, RpcError, RpcResult, ValidationError};
use ic_cdk::api::management_canister::http_request::{
    CanisterHttpRequestArgument, HttpHeader, HttpMethod, HttpResponse, TransformArgs,
    TransformContext,
};
use num_traits::ToPrimitive;
use tower::{BoxError, Service, ServiceBuilder};

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
    let _host = match parsed_url.host_str() {
        Some(host) => host,
        None => {
            return Err(ValidationError::Custom(format!(
                "Error parsing hostname from URL: {}",
                url
            ))
            .into())
        }
    };
    http_client(rpc_method).call(request).await
}

pub fn http_client(
    rpc_method: MetricRpcMethod,
) -> impl Service<CanisterHttpRequestArgument, Response = HttpResponse, Error = RpcError> {
    ServiceBuilder::new()
        .layer(
            ObservabilityLayer::new()
                .on_request(move |req: &CanisterHttpRequestArgument| {
                    let req_data = MetricData::new(rpc_method.clone(), req);
                    add_metric_entry!(
                        requests,
                        (req_data.method.clone(), req_data.host.clone()),
                        1
                    );
                    req_data
                })
                .on_response(|req_data: MetricData, response: &HttpResponse| {
                    let status: u32 = response.status.0.clone().try_into().unwrap_or(0);
                    add_metric_entry!(
                        responses,
                        (req_data.method, req_data.host, status.into()),
                        1
                    );
                })
                .on_error(|req_data: MetricData, error: &RpcError| {
                    if let RpcError::HttpOutcallError(HttpOutcallError::IcError {
                        code,
                        message: _,
                    }) = error
                    {
                        add_metric_entry!(
                            err_http_outcall,
                            (req_data.method, req_data.host, *code),
                            1
                        );
                    }
                }),
        )
        .map_err(map_error)
        .filter(CyclesAccounting::new(
            get_num_subnet_nodes(),
            ChargingPolicyWithCollateral::default(),
        ))
        .service(canhttp::Client)
}

struct MetricData {
    method: MetricRpcMethod,
    host: MetricRpcHost,
}

impl MetricData {
    pub fn new(method: MetricRpcMethod, request: &CanisterHttpRequestArgument) -> Self {
        Self {
            method,
            host: MetricRpcHost(
                url::Url::parse(&request.url)
                    .unwrap()
                    .host_str()
                    .unwrap()
                    .to_string(),
            ),
        }
    }
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

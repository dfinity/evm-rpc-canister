use crate::constants::COLLATERAL_CYCLES_PER_NODE;
use crate::memory::{get_num_subnet_nodes, is_demo_active};
use crate::{
    add_metric_entry,
    constants::CONTENT_TYPE_VALUE,
    memory::get_override_provider,
    types::{MetricRpcHost, MetricRpcMethod, ResolvedRpcService},
    util::canonicalize_json,
};
use canhttp::http::{
    HttpConversionLayer, MaxResponseBytesRequestExtension, TransformContextRequestExtension,
};
use canhttp::{
    observability::ObservabilityLayer, CyclesAccounting, CyclesAccountingError,
    CyclesChargingPolicy,
};
use evm_rpc_types::{HttpOutcallError, ProviderError, RpcError, RpcResult, ValidationError};
use http::header::CONTENT_TYPE;
use http::HeaderValue;
use ic_cdk::api::management_canister::http_request::{
    CanisterHttpRequestArgument, HttpHeader, HttpResponse, TransformArgs, TransformContext,
};
use tower::{BoxError, Service, ServiceBuilder};
use tower_http::ServiceBuilderExt;

pub fn json_rpc_request_arg(
    service: ResolvedRpcService,
    json_rpc_payload: &str,
    max_response_bytes: u64,
) -> RpcResult<canhttp::http::HttpRequest> {
    let api = service.api(&get_override_provider())?;
    let mut request_builder = http::Request::post(api.url)
        .max_response_bytes(max_response_bytes)
        .transform_context(TransformContext::from_name(
            "__transform_json_rpc".to_string(),
            vec![],
        ));
    for HttpHeader { name, value } in api.headers.unwrap_or_default() {
        request_builder = request_builder.header(name, value);
    }
    request_builder
        .body(json_rpc_payload.as_bytes().to_vec())
        .map_err(|e| {
            RpcError::ValidationError(ValidationError::Custom(format!("Invalid request: {e}")))
        })
}

pub async fn json_rpc_request(
    service: ResolvedRpcService,
    json_rpc_payload: &str,
    max_response_bytes: u64,
) -> RpcResult<canhttp::http::HttpResponse> {
    let request = json_rpc_request_arg(service, json_rpc_payload, max_response_bytes)?;
    canhttp_client(MetricRpcMethod("request".to_string()))
        .call(request)
        .await
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

pub fn canhttp_client(
    rpc_method: MetricRpcMethod,
) -> impl Service<canhttp::http::HttpRequest, Response = canhttp::http::HttpResponse, Error = RpcError>
{
    ServiceBuilder::new()
        .layer(
            ObservabilityLayer::new()
                .on_request(move |req: &canhttp::http::HttpRequest| {
                    let req_data = MetricData {
                        method: rpc_method.clone(),
                        host: MetricRpcHost(req.uri().host().unwrap().to_string()),
                    };
                    add_metric_entry!(
                        requests,
                        (req_data.method.clone(), req_data.host.clone()),
                        1
                    );
                    req_data
                })
                .on_response(
                    |req_data: MetricData, response: &canhttp::http::HttpResponse| {
                        let status: u32 = response.status().as_u16() as u32;
                        add_metric_entry!(
                            responses,
                            (req_data.method, req_data.host, status.into()),
                            1
                        );
                    },
                )
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
        .insert_request_header_if_not_present(
            CONTENT_TYPE,
            HeaderValue::from_static(CONTENT_TYPE_VALUE),
        )
        .layer(HttpConversionLayer)
        .filter(CyclesAccounting::new(
            get_num_subnet_nodes(),
            ChargingPolicyWithCollateral::default(),
        ))
        .service(canhttp::Client)
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

pub fn get_http_response_body(response: canhttp::http::HttpResponse) -> Result<String, RpcError> {
    let (parts, body) = response.into_parts();
    String::from_utf8(body).map_err(|e| {
        HttpOutcallError::InvalidHttpJsonRpcResponse {
            status: parts.status.as_u16(),
            body: "".to_string(),
            parsing_error: Some(format!("{e}")),
        }
        .into()
    })
}

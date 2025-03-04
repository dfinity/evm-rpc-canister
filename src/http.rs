use crate::constants::COLLATERAL_CYCLES_PER_NODE;
use crate::logs::TRACE_HTTP;
use crate::memory::{get_num_subnet_nodes, is_demo_active};
use crate::{
    add_metric_entry,
    constants::CONTENT_TYPE_VALUE,
    memory::get_override_provider,
    types::{MetricRpcHost, MetricRpcMethod, ResolvedRpcService},
    util::canonicalize_json,
};
use canhttp::http::json::{
    HttpJsonRpcRequest, HttpJsonRpcResponse, JsonRequestConversionError, JsonRequestConverter,
    JsonResponseConversionError, JsonResponseConverter, JsonRpcRequestBody,
};
use canhttp::http::{
    FilterNonSuccessfulHttpResponse, FilterNonSuccessulHttpResponseError,
    HttpRequestConversionError, HttpRequestConverter, HttpResponseConversionError,
    HttpResponseConverter, MaxResponseBytesRequestExtension, TransformContextRequestExtension,
};
use canhttp::{
    observability::ObservabilityLayer, ConvertRequestLayer, ConvertServiceBuilder,
    CyclesAccounting, CyclesAccountingError, CyclesChargingPolicy, IcError,
};
use evm_rpc_types::{HttpOutcallError, ProviderError, RpcError, RpcResult, ValidationError};
use http::header::CONTENT_TYPE;
use http::HeaderValue;
use ic_canister_log::log;
use ic_cdk::api::management_canister::http_request::{
    CanisterHttpRequestArgument as IcHttpRequest, HttpResponse as IcHttpResponse, TransformArgs,
    TransformContext,
};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::fmt::Debug;
use thiserror::Error;
use tower::layer::util::{Identity, Stack};
use tower::{BoxError, Service, ServiceBuilder};
use tower_http::set_header::SetRequestHeaderLayer;
use tower_http::ServiceBuilderExt;

pub fn json_rpc_request_arg(
    service: ResolvedRpcService,
    json_rpc_payload: &str,
    max_response_bytes: u64,
) -> RpcResult<HttpJsonRpcRequest<serde_json::Value>> {
    let body: JsonRpcRequestBody<serde_json::Value> = serde_json::from_str(json_rpc_payload)
        .map_err(|e| {
            RpcError::ValidationError(ValidationError::Custom(format!(
                "Invalid JSON RPC request: {e}"
            )))
        })?;
    service
        .post(&get_override_provider())?
        .max_response_bytes(max_response_bytes)
        .transform_context(TransformContext::from_name(
            "__transform_json_rpc".to_string(),
            vec![],
        ))
        .body(body)
        .map_err(|e| {
            RpcError::ValidationError(ValidationError::Custom(format!("Invalid request: {e}")))
        })
}

pub async fn json_rpc_request(
    service: ResolvedRpcService,
    json_rpc_payload: &str,
    max_response_bytes: u64,
) -> RpcResult<HttpJsonRpcResponse<serde_json::Value>> {
    let request = json_rpc_request_arg(service, json_rpc_payload, max_response_bytes)?;
    http_client(MetricRpcMethod("request".to_string()))
        .call(request)
        .await
}

pub fn http_client<I, O>(
    rpc_method: MetricRpcMethod,
) -> impl Service<HttpJsonRpcRequest<I>, Response = HttpJsonRpcResponse<O>, Error = RpcError>
where
    I: Serialize,
    O: DeserializeOwned + Debug,
{
    ServiceBuilder::new()
        .map_err(|e: HttpClientError| RpcError::from(e))
        .layer(
            ObservabilityLayer::new()
                .on_request(move |req: &HttpJsonRpcRequest<I>| {
                    let req_data = MetricData {
                        method: rpc_method.clone(),
                        host: MetricRpcHost(req.uri().host().unwrap().to_string()),
                        request_id: req.body().id().cloned(),
                    };
                    add_metric_entry!(
                        requests,
                        (req_data.method.clone(), req_data.host.clone()),
                        1
                    );
                    req_data
                })
                .on_response(|req_data: MetricData, response: &HttpJsonRpcResponse<O>| {
                    observe_response(req_data.method, req_data.host, response.status().as_u16());
                    log!(
                        TRACE_HTTP,
                        "Got response for request with id `{:?}`. Response with status {}: {:?}",
                        req_data.request_id,
                        response.status(),
                        response.body()
                    );
                })
                .on_error(
                    |req_data: MetricData, error: &HttpClientError| match error {
                        HttpClientError::IcError(IcError { code, message: _ }) => {
                            add_metric_entry!(
                                err_http_outcall,
                                (req_data.method, req_data.host, *code),
                                1
                            );
                        }
                        HttpClientError::UnsuccessfulHttpResponse(
                            FilterNonSuccessulHttpResponseError::UnsuccessfulResponse(response),
                        ) => {
                            observe_response(
                                req_data.method,
                                req_data.host,
                                response.status().as_u16(),
                            );
                            log!(
                                TRACE_HTTP,
                                "Unsuccessful HTTP response for request with id `{:?}`. Response with status {}: {}",
                                req_data.request_id,
                                response.status(),
                                String::from_utf8_lossy(response.body())
                            );
                        }
                        HttpClientError::InvalidJsonResponse(
                            JsonResponseConversionError::InvalidJsonResponse {
                                status,
                                body: _,
                                parsing_error: _,
                            },
                        ) => {
                            observe_response(req_data.method, req_data.host, *status);
                            log!(
                                TRACE_HTTP,
                                "Invalid JSON RPC response for request with id `{:?}`: {}",
                                req_data.request_id,
                                error
                            );
                        }
                        HttpClientError::InvalidRequest(_) => {}
                        HttpClientError::CyclesAccountingError(_) => {}
                        HttpClientError::InvalidHttpResponse(_) => {}
                    },
                ),
        )
        .layer(service_request_builder())
        .convert_response(JsonResponseConverter::new())
        .convert_response(FilterNonSuccessfulHttpResponse)
        .convert_response(HttpResponseConverter)
        .convert_request(CyclesAccounting::new(
            get_num_subnet_nodes(),
            ChargingPolicyWithCollateral::default(),
        ))
        .service(canhttp::Client::new_with_error::<HttpClientError>())
}

fn observe_response(method: MetricRpcMethod, host: MetricRpcHost, status: u16) {
    let status: u32 = status as u32;
    add_metric_entry!(responses, (method, host, status.into()), 1);
}

type JsonRpcServiceBuilder = ServiceBuilder<
    Stack<
        ConvertRequestLayer<HttpRequestConverter>,
        Stack<
            ConvertRequestLayer<JsonRequestConverter>,
            Stack<SetRequestHeaderLayer<HeaderValue>, Identity>,
        >,
    >,
>;

/// Middleware that takes care of transforming the request.
///
/// It's required to separate it from the other middlewares, to compute the exact request cost.
pub fn service_request_builder() -> JsonRpcServiceBuilder {
    ServiceBuilder::new()
        .insert_request_header_if_not_present(
            CONTENT_TYPE,
            HeaderValue::from_static(CONTENT_TYPE_VALUE),
        )
        .convert_request(JsonRequestConverter)
        .convert_request(HttpRequestConverter)
}

#[derive(Error, Debug)]
pub enum HttpClientError {
    #[error("IC error: {0}")]
    IcError(IcError),
    #[error("Request is invalid: {0}")]
    InvalidRequest(BoxError),
    #[error("cycles accounting error: {0}")]
    CyclesAccountingError(CyclesAccountingError),
    #[error("Error converting to http::Response: {0}")]
    InvalidHttpResponse(HttpResponseConversionError),
    #[error("HTTP response was not successful: {0}")]
    UnsuccessfulHttpResponse(FilterNonSuccessulHttpResponseError<Vec<u8>>),
    #[error("Error converting response to JSON: {0}")]
    InvalidJsonResponse(JsonResponseConversionError),
}

impl From<IcError> for HttpClientError {
    fn from(value: IcError) -> Self {
        HttpClientError::IcError(value)
    }
}

impl From<HttpResponseConversionError> for HttpClientError {
    fn from(value: HttpResponseConversionError) -> Self {
        HttpClientError::InvalidHttpResponse(value)
    }
}

impl From<FilterNonSuccessulHttpResponseError<Vec<u8>>> for HttpClientError {
    fn from(value: FilterNonSuccessulHttpResponseError<Vec<u8>>) -> Self {
        HttpClientError::UnsuccessfulHttpResponse(value)
    }
}

impl From<JsonResponseConversionError> for HttpClientError {
    fn from(value: JsonResponseConversionError) -> Self {
        HttpClientError::InvalidJsonResponse(value)
    }
}

impl From<CyclesAccountingError> for HttpClientError {
    fn from(value: CyclesAccountingError) -> Self {
        HttpClientError::CyclesAccountingError(value)
    }
}

impl From<HttpRequestConversionError> for HttpClientError {
    fn from(value: HttpRequestConversionError) -> Self {
        HttpClientError::InvalidRequest(value.into())
    }
}

impl From<JsonRequestConversionError> for HttpClientError {
    fn from(value: JsonRequestConversionError) -> Self {
        HttpClientError::InvalidRequest(value.into())
    }
}

impl From<HttpClientError> for RpcError {
    fn from(error: HttpClientError) -> Self {
        match error {
            HttpClientError::IcError(IcError { code, message }) => {
                RpcError::HttpOutcallError(HttpOutcallError::IcError { code, message })
            }
            HttpClientError::InvalidRequest(e) => {
                RpcError::ValidationError(ValidationError::Custom(e.to_string()))
            }
            HttpClientError::CyclesAccountingError(
                CyclesAccountingError::InsufficientCyclesError { expected, received },
            ) => RpcError::ProviderError(ProviderError::TooFewCycles { expected, received }),
            HttpClientError::InvalidHttpResponse(e) => {
                RpcError::ValidationError(ValidationError::Custom(e.to_string()))
            }
            HttpClientError::InvalidJsonResponse(
                JsonResponseConversionError::InvalidJsonResponse {
                    status,
                    body,
                    parsing_error,
                },
            ) => RpcError::HttpOutcallError(HttpOutcallError::InvalidHttpJsonRpcResponse {
                status,
                body,
                parsing_error: Some(parsing_error),
            }),
            HttpClientError::UnsuccessfulHttpResponse(
                FilterNonSuccessulHttpResponseError::UnsuccessfulResponse(response),
            ) => RpcError::HttpOutcallError(HttpOutcallError::InvalidHttpJsonRpcResponse {
                status: response.status().as_u16(),
                body: String::from_utf8_lossy(response.body()).to_string(),
                parsing_error: None,
            }),
        }
    }
}

struct MetricData {
    method: MetricRpcMethod,
    host: MetricRpcHost,
    request_id: Option<serde_json::Value>,
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
    fn cycles_to_charge(&self, _request: &IcHttpRequest, attached_cycles: u128) -> u128 {
        if self.charge_user {
            return attached_cycles.saturating_add(self.collateral_cycles);
        }
        0
    }
}

pub fn transform_http_request(args: TransformArgs) -> IcHttpResponse {
    IcHttpResponse {
        status: args.response.status,
        body: canonicalize_json(&args.response.body).unwrap_or(args.response.body),
        // Remove headers (which may contain a timestamp) for consensus
        headers: vec![],
    }
}

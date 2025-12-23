use crate::{
    constants::{COLLATERAL_CYCLES_PER_NODE, CONTENT_TYPE_VALUE},
    memory::{get_num_subnet_nodes, is_demo_active, next_request_id},
    util::canonicalize_json,
};
use canhttp::cycles::ChargeCallerError;
use canhttp::http::json::{
    ConsistentResponseIdFilterError, CreateJsonRpcIdFilter, HttpBatchJsonRpcRequest,
    JsonRequestConversionError, JsonResponseConversionError, JsonRpcRequest,
};
use canhttp::http::{
    FilterNonSuccessfulHttpResponseError, HttpRequestConversionError, HttpResponseConversionError,
};
use canhttp::observability::ObservabilityLayer;
use canhttp::{
    convert::ConvertRequestLayer,
    cycles::{ChargeCaller, CyclesAccounting},
    http::{
        json::{HttpJsonRpcRequest, JsonRequestConverter, JsonResponseConverter, JsonRpcCall},
        FilterNonSuccessfulHttpResponse, HttpRequestConverter, HttpResponseConverter,
    },
    retry::DoubleMaxResponseBytes,
    ConvertServiceBuilder, HttpsOutcallError, IcError,
};
use evm_rpc_types::RpcError;
use http::{header::CONTENT_TYPE, HeaderValue, Request as HttpRequest, Response as HttpResponse};
use ic_cdk::management_canister::{
    HttpRequestArgs as IcHttpRequest, HttpRequestResult as IcHttpResponse, TransformArgs,
};
use observability::ObserveHttpCall;
use serde::{de::DeserializeOwned, Serialize};
use tower::{
    layer::util::{Identity, Stack},
    retry::RetryLayer,
    util::MapRequestLayer,
    Service, ServiceBuilder,
};
use tower_http::{set_header::SetRequestHeaderLayer, ServiceBuilderExt};

pub mod error;
pub mod legacy;
mod observability;

pub fn client<Request, Response, Error>(
    retry: bool,
) -> impl Service<HttpRequest<Request>, Response = HttpResponse<Response>, Error = RpcError>
where
    HttpRequest<Request>: GenerateRequestId,
    (Request, Response): JsonRpcCall<Request, Response>,
    (Request, Response, Error): ObserveHttpCall<Request, Response, Error>,
    Request: Serialize + Clone,
    Response: DeserializeOwned,
    RpcError: From<Error>,
    Error: From<IcError>
        + From<JsonRequestConversionError>
        + From<HttpRequestConversionError>
        + From<ChargeCallerError>
        + From<HttpResponseConversionError>
        + From<FilterNonSuccessfulHttpResponseError<Vec<u8>>>
        + From<JsonResponseConversionError>
        + From<ConsistentResponseIdFilterError>
        + HttpsOutcallError,
{
    let maybe_retry = if retry {
        Some(RetryLayer::new(DoubleMaxResponseBytes))
    } else {
        None
    };
    let maybe_unique_id = if retry {
        Some(MapRequestLayer::new(|request: HttpRequest<Request>| {
            request.generate_request_id()
        }))
    } else {
        None
    };
    ServiceBuilder::new()
        .map_err(|e: Error| RpcError::from(e))
        .option_layer(maybe_retry)
        .option_layer(maybe_unique_id)
        .layer(
            ObservabilityLayer::new()
                .on_request(<(Request, Response, Error)>::observe_request)
                .on_response(<(Request, Response, Error)>::observe_response)
                .on_error(<(Request, Response, Error)>::observe_error),
        )
        .filter_response(CreateJsonRpcIdFilter::new())
        .layer(service_request_builder())
        .convert_response(JsonResponseConverter::new())
        .convert_response(FilterNonSuccessfulHttpResponse)
        .convert_response(HttpResponseConverter)
        .convert_request(CyclesAccounting::new(charging_policy_with_collateral()))
        .service(canhttp::Client::new_with_error::<Error>())
}

pub trait GenerateRequestId: Sized {
    fn generate_request_id(self) -> Self;
}

impl<I> GenerateRequestId for HttpBatchJsonRpcRequest<I> {
    fn generate_request_id(self) -> Self {
        self.map(|requests| {
            requests
                .into_iter()
                .map(|mut request: JsonRpcRequest<I>| {
                    request.set_id(next_request_id());
                    request
                })
                .collect()
        })
    }
}

impl<I> GenerateRequestId for HttpJsonRpcRequest<I> {
    fn generate_request_id(self) -> Self {
        self.map(|mut request: JsonRpcRequest<I>| {
            request.set_id(next_request_id());
            request
        })
    }
}

type JsonRpcServiceBuilder<I> = ServiceBuilder<
    Stack<
        ConvertRequestLayer<HttpRequestConverter>,
        Stack<
            ConvertRequestLayer<JsonRequestConverter<I>>,
            Stack<SetRequestHeaderLayer<HeaderValue>, Identity>,
        >,
    >,
>;

/// Middleware that takes care of transforming the request.
///
/// It's required to separate it from the other middlewares, to compute the exact request cost.
pub fn service_request_builder<Request>() -> JsonRpcServiceBuilder<Request> {
    ServiceBuilder::new()
        .insert_request_header_if_not_present(
            CONTENT_TYPE,
            HeaderValue::from_static(CONTENT_TYPE_VALUE),
        )
        .convert_request(JsonRequestConverter::<Request>::new())
        .convert_request(HttpRequestConverter)
}

pub fn charging_policy_with_collateral(
) -> ChargeCaller<impl Fn(&IcHttpRequest, u128) -> u128 + Clone> {
    let charge_caller = if is_demo_active() {
        |_request: &IcHttpRequest, _request_cost| 0
    } else {
        |_request: &IcHttpRequest, request_cost| {
            let collateral_cycles =
                COLLATERAL_CYCLES_PER_NODE.saturating_mul(get_num_subnet_nodes() as u128);
            request_cost + collateral_cycles
        }
    };
    ChargeCaller::new(charge_caller)
}

pub fn transform_http_request(args: TransformArgs) -> IcHttpResponse {
    IcHttpResponse {
        status: args.response.status,
        body: canonicalize_json(&args.response.body).unwrap_or(args.response.body),
        // Remove headers (which may contain a timestamp) for consensus
        headers: vec![],
    }
}

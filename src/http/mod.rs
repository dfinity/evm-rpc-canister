use crate::{
    constants::{COLLATERAL_CYCLES_PER_NODE, CONTENT_TYPE_VALUE},
    memory::{get_num_subnet_nodes, is_demo_active, next_request_id},
    util::canonicalize_json,
};
use canhttp::http::json::{CreateJsonRpcIdFilter, HttpBatchJsonRpcRequest, JsonRpcRequest};
use canhttp::observability::ObservabilityLayer;
use canhttp::{
    convert::ConvertRequestLayer,
    cycles::{ChargeCaller, CyclesAccounting},
    http::{
        json::{HttpJsonRpcRequest, JsonRequestConverter, JsonResponseConverter, JsonRpcCall},
        FilterNonSuccessfulHttpResponse, HttpRequestConverter, HttpResponseConverter,
    },
    retry::DoubleMaxResponseBytes,
    ConvertServiceBuilder,
};
use error::HttpClientError;
use evm_rpc_types::RpcError;
use http::{header::CONTENT_TYPE, HeaderValue, Request as HttpRequest, Response as HttpResponse};
use ic_cdk::management_canister::{
    HttpRequestArgs as IcHttpRequest, HttpRequestResult as IcHttpResponse, TransformArgs,
};
use observability::ObserveHttpCall;
use serde::{de::DeserializeOwned, Serialize};
use std::fmt::Debug;
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

pub fn client<Request, Response>(
    retry: bool,
) -> impl Service<HttpRequest<Request>, Response = HttpResponse<Response>, Error = RpcError>
where
    HttpRequest<Request>: GenerateRequestId,
    (Request, Response): JsonRpcCall<Request, Response>,
    (Request, Response, HttpClientError): ObserveHttpCall<Request, Response, HttpClientError>,
    Request: Debug + Serialize + Clone,
    Response: Debug + DeserializeOwned,
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
        .map_err(|e: HttpClientError| RpcError::from(e))
        .option_layer(maybe_retry)
        .option_layer(maybe_unique_id)
        .layer(
            ObservabilityLayer::new()
                .on_request(<(Request, Response, HttpClientError)>::observe_request)
                .on_response(<(Request, Response, HttpClientError)>::observe_response)
                .on_error(<(Request, Response, HttpClientError)>::observe_error),
        )
        .filter_response(CreateJsonRpcIdFilter::new())
        .layer(service_request_builder())
        .convert_response(JsonResponseConverter::new())
        .convert_response(FilterNonSuccessfulHttpResponse)
        .convert_response(HttpResponseConverter)
        .convert_request(CyclesAccounting::new(charging_policy_with_collateral()))
        .service(canhttp::Client::new_with_error::<HttpClientError>())
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
pub fn service_request_builder<I>() -> JsonRpcServiceBuilder<I> {
    ServiceBuilder::new()
        .insert_request_header_if_not_present(
            CONTENT_TYPE,
            HeaderValue::from_static(CONTENT_TYPE_VALUE),
        )
        .convert_request(JsonRequestConverter::<I>::new())
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

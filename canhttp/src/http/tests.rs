use crate::http::{
    HttpRequestConversionLayer, HttpRequestFilterError, HttpResponseConversionLayer,
    MaxResponseBytesRequestExtensionBuilder, TransformContextRequestExtensionBuilder,
};
use crate::IcError;
use assert_matches::assert_matches;
use candid::Principal;
use ic_cdk::api::management_canister::http_request::{
    CanisterHttpRequestArgument as IcHttpRequest, HttpHeader as IcHttpHeader,
    HttpMethod as IcHttpMethod,
};
use ic_cdk::api::management_canister::http_request::{TransformContext, TransformFunc};
use tower::{Service, ServiceBuilder, ServiceExt};

#[tokio::test]
async fn should_convert_http_request() {
    let url = "https://internetcomputer.org/";
    let max_response_bytes = 1_000;
    let transform_context = TransformContext {
        function: TransformFunc::new(Principal::management_canister(), "sanitize".to_string()),
        context: vec![35_u8; 20],
    };
    let body = vec![42_u8; 32];

    let mut service = ServiceBuilder::new()
        .layer(HttpRequestConversionLayer)
        .service_fn(echo_request);

    for (request_builder, expected_http_method) in [
        (http::Request::post(url), IcHttpMethod::POST),
        (http::Request::get(url), IcHttpMethod::GET),
        (http::Request::head(url), IcHttpMethod::HEAD),
    ] {
        let request = request_builder
            .max_response_bytes(max_response_bytes)
            .transform_context(transform_context.clone())
            .header("Content-Type", "application/json")
            .body(body.clone())
            .unwrap();

        let converted_request = service.ready().await.unwrap().call(request).await.unwrap();

        assert_eq!(
            converted_request,
            IcHttpRequest {
                url: url.to_string(),
                max_response_bytes: Some(max_response_bytes),
                method: expected_http_method,
                headers: vec![IcHttpHeader {
                    name: "content-type".to_string(),
                    value: "application/json".to_string()
                }],
                body: Some(body.clone()),
                transform: Some(transform_context.clone()),
            }
        )
    }
}

#[tokio::test]
async fn should_fail_when_http_method_unsupported() {
    let mut service = ServiceBuilder::new()
        .layer(HttpRequestConversionLayer)
        .service_fn(echo_request);
    let url = "https://internetcomputer.org/";

    for request_builder in [
        http::Request::connect(url),
        http::Request::delete(url),
        http::Request::patch(url),
        http::Request::put(url),
        http::Request::options(url),
        http::Request::trace(url),
    ] {
        let unsupported_request = request_builder.body(vec![]).unwrap();

        let error = service
            .ready()
            .await
            .unwrap()
            .call(unsupported_request)
            .await
            .expect_err("BUG: method should be unsupported")
            .downcast_ref::<HttpRequestFilterError>()
            .expect("BUG: unexpected error type")
            .clone();

        assert_matches!(error, HttpRequestFilterError::UnsupportedHttpMethod(_));
    }
}

async fn echo_request(request: IcHttpRequest) -> Result<IcHttpRequest, IcError> {
    Ok(request)
}

use crate::http::HttpConversionLayer;
use crate::IcError;
use assert_matches::assert_matches;
use candid::{Decode, Encode, Principal};
use http::StatusCode;
use ic_cdk::api::management_canister::http_request::{
    CanisterHttpRequestArgument as IcHttpRequest, HttpHeader as IcHttpHeader,
    HttpMethod as IcHttpMethod, HttpResponse as IcHttpResponse,
};
use ic_cdk::api::management_canister::http_request::{TransformContext, TransformFunc};
use tower::{Service, ServiceBuilder, ServiceExt};
use crate::http::request::{HttpRequestConversionError, HttpRequestConversionLayer, MaxResponseBytesRequestExtensionBuilder, TransformContextRequestExtensionBuilder};
use crate::http::response::{HttpResponse, HttpResponseConversionError, HttpResponseConversionLayer};

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
            .downcast_ref::<HttpRequestConversionError>()
            .expect("BUG: unexpected error type")
            .clone();

        assert_matches!(error, HttpRequestConversionError::UnsupportedHttpMethod(_));
    }
}

#[tokio::test]
async fn should_convert_http_response() {
    let mut service = ServiceBuilder::new()
        .layer(HttpResponseConversionLayer)
        .service_fn(echo_response);

    let response = IcHttpResponse {
        status: 200_u8.into(),
        headers: vec![IcHttpHeader {
            name: "content-type".to_string(),
            value: "application/json".to_string(),
        }],
        body: vec![42; 32],
    };

    let converted_response = service.ready().await.unwrap().call(response).await.unwrap();

    assert_response_eq(
        converted_response,
        http::Response::builder()
            .status(200)
            .header("content-type", "application/json")
            .body(vec![42; 32])
            .unwrap(),
    )
}

#[tokio::test]
async fn should_fail_to_convert_http_response() {
    let mut service = ServiceBuilder::new()
        .layer(HttpResponseConversionLayer)
        .service_fn(echo_response);

    let response = IcHttpResponse {
        status: 99_u8.into(),
        headers: vec![IcHttpHeader {
            name: "content-type".to_string(),
            value: "application/json".to_string(),
        }],
        body: vec![42; 32],
    };

    let error = service
        .ready()
        .await
        .unwrap()
        .call(response)
        .await
        .expect_err("BUG: method should be unsupported")
        .downcast_ref::<HttpResponseConversionError>()
        .expect("BUG: unexpected error type")
        .clone();

    assert_eq!(error, HttpResponseConversionError::InvalidStatusCode);
}

#[tokio::test]
async fn should_convert_both_request_and_responses() {
    async fn serialize_request_and_add_header(
        request: IcHttpRequest,
    ) -> Result<IcHttpResponse, IcError> {
        Ok(IcHttpResponse {
            status: 200_u8.into(),
            headers: vec![IcHttpHeader {
                name: "from_response_name".to_string(),
                value: "from_response_value".to_string(),
            }],
            body: Encode!(&request).unwrap(),
        })
    }

    let mut service = ServiceBuilder::new()
        .layer(HttpConversionLayer)
        .service_fn(serialize_request_and_add_header);

    let url = "https://internetcomputer.org/";
    let max_response_bytes = 1_000;
    let transform_context = TransformContext {
        function: TransformFunc::new(Principal::management_canister(), "sanitize".to_string()),
        context: vec![35_u8; 20],
    };
    let body = vec![42_u8; 32];
    let request = http::Request::post(url)
        .max_response_bytes(max_response_bytes)
        .transform_context(transform_context.clone())
        .header("Content-Type", "application/json")
        .body(body.clone())
        .unwrap();

    let response = service.ready().await.unwrap().call(request).await.unwrap();
    let converted_request = Decode!(response.body(), IcHttpRequest).unwrap();

    assert_eq!(
        converted_request,
        IcHttpRequest {
            url: url.to_string(),
            max_response_bytes: Some(max_response_bytes),
            method: IcHttpMethod::POST,
            headers: vec![IcHttpHeader {
                name: "content-type".to_string(),
                value: "application/json".to_string()
            }],
            body: Some(body.clone()),
            transform: Some(transform_context.clone()),
        }
    );

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers().len(), 1);
    assert_eq!(
        response.headers().get("from_response_name"),
        Some(&"from_response_value".parse().unwrap())
    )
}

async fn echo_request(request: IcHttpRequest) -> Result<IcHttpRequest, IcError> {
    Ok(request)
}

async fn echo_response(response: IcHttpResponse) -> Result<IcHttpResponse, IcError> {
    Ok(response)
}

// http::Response<T> does not implement PartialEq
fn assert_response_eq(left: HttpResponse, right: HttpResponse) {
    let (left_parts, left_body) = left.into_parts();
    let (right_parts, right_body) = right.into_parts();

    assert_eq!(left_body, right_body);
    assert_eq!(left_parts.status, right_parts.status);
    assert_eq!(left_parts.version, right_parts.version);
    assert_eq!(left_parts.headers, right_parts.headers);

    // There is no-way to check the full content of the extensions,
    // so we just ensure that both are empty
    assert!(left_parts.extensions.is_empty());
    assert!(right_parts.extensions.is_empty());
}

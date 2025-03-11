use crate::http::json::{JsonConversionLayer, JsonRequestConverter, JsonResponseConverter};
use crate::http::{HttpRequest, HttpResponse};
use crate::ConvertServiceBuilder;
use http::HeaderValue;
use serde_json::json;
use tower::{BoxError, Service, ServiceBuilder, ServiceExt};

mod json_rpc {
    use crate::http::json::Id;
    use serde_json::json;

    #[test]
    fn should_parse_null_id() {
        let id: Id = serde_json::from_value(json!(null)).unwrap();
        assert_eq!(id, Id::Null);
    }

    #[test]
    fn should_parse_numeric_id() {
        let id: Id = serde_json::from_value(json!(42)).unwrap();
        assert_eq!(id, Id::Number(42));
    }

    #[test]
    fn should_parse_string_id() {
        let id: Id = serde_json::from_value(json!("forty two")).unwrap();
        assert_eq!(id, Id::String("forty two".into()));
    }

    #[test]
    fn should_fail_to_parse_id_from_wrong_types() {
        for value in [json!(true), json!(["array"]), json!({"an": "object"})] {
            let _error = serde_json::from_value::<Id>(value).expect_err("should fail");
        }
    }
}

#[tokio::test]
async fn should_convert_json_request() {
    let url = "https://internetcomputer.org/";
    let mut service = ServiceBuilder::new()
        .convert_request(JsonRequestConverter::<serde_json::Value>::new())
        .service_fn(echo_request);

    let body = json!({"foo": "bar"});
    let request = http::Request::post(url).body(body.clone()).unwrap();

    let converted_request = service.ready().await.unwrap().call(request).await.unwrap();

    assert_eq!(
        serde_json::to_vec(&body).unwrap(),
        converted_request.into_body()
    );
}

#[tokio::test]
async fn should_add_content_type_header_if_missing() {
    let url = "https://internetcomputer.org/";
    let mut service = ServiceBuilder::new()
        .convert_request(JsonRequestConverter::<serde_json::Value>::new())
        .service_fn(echo_request);

    for (request_content_type, expected_content_type) in [
        (None, "application/json"),
        (Some("wrong-value"), "wrong-value"), //do not overwrite explicitly set header
        (Some("application/json"), "application/json"),
    ] {
        let mut builder = http::Request::post(url);
        if let Some(request_content_type) = request_content_type {
            builder = builder.header(http::header::CONTENT_TYPE, request_content_type);
        }
        let request = builder
            .header("other-header", "should-remain")
            .body(json!({"foo": "bar"}))
            .unwrap();

        let converted_request = service
            .ready()
            .await
            .unwrap()
            .call(request.clone())
            .await
            .unwrap();

        let (mut request_parts, _body) = request.into_parts();
        let (mut converted_request_parts, _body) = converted_request.into_parts();

        assert_eq!(request_parts.method, converted_request_parts.method);
        assert_eq!(request_parts.uri, converted_request_parts.uri);
        assert_eq!(request_parts.version, converted_request_parts.version);

        // Headers should be identical, excepted for content-type
        request_parts.headers.remove(http::header::CONTENT_TYPE);
        let converted_request_content_type = converted_request_parts
            .headers
            .remove(http::header::CONTENT_TYPE)
            .unwrap();
        assert_eq!(
            converted_request_content_type,
            HeaderValue::from_static(expected_content_type)
        );

        assert_eq!(request_parts.headers, converted_request_parts.headers);
    }
}

#[tokio::test]
async fn should_convert_json_response() {
    let mut service = ServiceBuilder::new()
        .convert_response(JsonResponseConverter::<serde_json::Value>::new())
        .service_fn(echo_response);

    let expected_response = json!({"foo": "bar"});
    let response = http::Response::new(serde_json::to_vec(&expected_response).unwrap());

    let converted_response = service.ready().await.unwrap().call(response).await.unwrap();

    assert_eq!(converted_response.into_body(), expected_response);
}

#[tokio::test]
async fn should_convert_both_request_and_response() {
    let mut service = ServiceBuilder::new()
        .layer(JsonConversionLayer::<serde_json::Value, serde_json::Value>::new())
        .service_fn(forward_body);

    let response = service
        .ready()
        .await
        .unwrap()
        .call(
            http::Request::post("https://internetcomputer.org/")
                .body(json!({"foo": "bar"}))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.into_body(), json!({"foo": "bar"}));
}

async fn echo_request(request: HttpRequest) -> Result<HttpRequest, BoxError> {
    Ok(request)
}

async fn echo_response(response: HttpResponse) -> Result<HttpResponse, BoxError> {
    Ok(response)
}

async fn forward_body(request: HttpRequest) -> Result<HttpResponse, BoxError> {
    Ok(http::Response::new(request.into_body()))
}

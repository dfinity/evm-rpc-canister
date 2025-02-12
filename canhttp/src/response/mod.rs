use bytes::Bytes;
use http::{HeaderMap, HeaderName, HeaderValue};
use ic_cdk::api::management_canister::http_request::{
    HttpHeader as IcHttpHeader, HttpResponse as IcHttpResponse,
};
use num_traits::ToPrimitive;

/// Map an [`IcHttpResponse`] to a `http::Response<Bytes>`.
pub fn map_ic_http_response(response: IcHttpResponse) -> http::Response<Bytes> {
    let mut builder = http::Response::builder().status(
        response
            .status
            .0
            .to_u16()
            .expect("BUG: invalid HTTP status code"),
    );
    if let Some(headers) = builder.headers_mut() {
        let mut response_headers = HeaderMap::with_capacity(response.headers.len());
        for IcHttpHeader { name, value } in response.headers {
            response_headers.insert(
                HeaderName::try_from(name).unwrap(),
                HeaderValue::try_from(value).unwrap(),
            );
        }
        headers.extend(response_headers);
    }

    builder.body(response.body.into()).unwrap()
}

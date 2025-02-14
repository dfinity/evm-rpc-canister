use bytes::Bytes;
use http::{HeaderMap, HeaderName, HeaderValue};
use http_body::Frame;
use ic_cdk::api::management_canister::http_request::{
    HttpHeader as IcHttpHeader, HttpResponse as IcHttpResponse,
};
use num_traits::ToPrimitive;
use std::convert::Infallible;
use std::pin::Pin;
use std::task::{Context, Poll};

/// TODO
pub type HttpResponse = http::Response<FullBytes>;

/// Similar to `http_body_util::Full<Bytes>`, but allow to retrieve the bytes synchronously.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FullBytes(Bytes);

impl Default for FullBytes {
    fn default() -> Self {
        FullBytes(Bytes::default())
    }
}

impl From<Vec<u8>> for FullBytes {
    fn from(value: Vec<u8>) -> Self {
        Self(value.into())
    }
}

impl From<FullBytes> for Vec<u8> {
    fn from(value: FullBytes) -> Self {
        value.0.into()
    }
}

impl From<String> for FullBytes {
    fn from(s: String) -> Self {
        Self(Bytes::from(s.into_bytes()))
    }
}

impl http_body::Body for FullBytes {
    type Data = Bytes;
    type Error = Infallible;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        let s = std::mem::take(&mut *self);
        Poll::Ready(Some(Ok(Frame::data(s.0))))
    }
}

/// Map an [`IcHttpResponse`] to a `http::Response<Bytes>`.
pub fn map_ic_http_response(response: IcHttpResponse) -> HttpResponse {
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

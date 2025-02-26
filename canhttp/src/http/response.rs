use ic_cdk::api::management_canister::http_request::HttpResponse as IcHttpResponse;
use pin_project::pin_project;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use thiserror::Error;
use tower::{BoxError, Service};
use tower_layer::Layer;

pub type HttpResponse = http::Response<Vec<u8>>;

#[derive(Error, Clone, Debug, Eq, PartialEq)]
#[allow(clippy::enum_variant_names)] //current variants reflect invalid data and so start with the prefix Invalid.
pub enum HttpResponseConversionError {
    #[error("Status code is invalid")]
    InvalidStatusCode,
    #[error("HTTP header `{name}` is invalid: {reason}")]
    InvalidHttpHeaderName { name: String, reason: String },
    #[error("HTTP header `{name}` has an invalid value: {reason}")]
    InvalidHttpHeaderValue { name: String, reason: String },
}

fn try_map_http_response(
    response: IcHttpResponse,
) -> Result<HttpResponse, HttpResponseConversionError> {
    use http::{HeaderMap, HeaderName, HeaderValue, StatusCode};
    use ic_cdk::api::management_canister::http_request::HttpHeader as IcHttpHeader;
    use num_traits::ToPrimitive;

    let status = response
        .status
        .0
        .to_u16()
        .and_then(|s| StatusCode::try_from(s).ok())
        .ok_or(HttpResponseConversionError::InvalidStatusCode)?;

    let mut builder = http::Response::builder().status(status);
    if let Some(headers) = builder.headers_mut() {
        let mut response_headers = HeaderMap::with_capacity(response.headers.len());
        for IcHttpHeader { name, value } in response.headers {
            response_headers.insert(
                HeaderName::try_from(&name).map_err(|e| {
                    HttpResponseConversionError::InvalidHttpHeaderName {
                        name: name.clone(),
                        reason: e.to_string(),
                    }
                })?,
                HeaderValue::try_from(&value).map_err(|e| {
                    HttpResponseConversionError::InvalidHttpHeaderValue {
                        name,
                        reason: e.to_string(),
                    }
                })?,
            );
        }
        headers.extend(response_headers);
    }

    Ok(builder
        .body(response.body)
        .expect("BUG: builder should have been modified only with validated data"))
}

pub struct HttpResponseConversionLayer;

pub struct HttpResponseConversion<S> {
    inner: S,
}

impl<S> Layer<S> for HttpResponseConversionLayer {
    type Service = HttpResponseConversion<S>;

    fn layer(&self, inner: S) -> Self::Service {
        Self::Service { inner }
    }
}

impl<S, Request, Error> Service<Request> for HttpResponseConversion<S>
where
    S: Service<Request, Response = IcHttpResponse, Error = Error>,
    Error: Into<BoxError>,
{
    type Response = HttpResponse;
    type Error = BoxError;
    type Future = ResponseFuture<S::Future>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx).map_err(Into::into)
    }

    fn call(&mut self, req: Request) -> Self::Future {
        ResponseFuture {
            response_future: self.inner.call(req),
        }
    }
}

#[pin_project]
pub struct ResponseFuture<F> {
    #[pin]
    response_future: F,
}

impl<F, E> Future for ResponseFuture<F>
where
    F: Future<Output = Result<IcHttpResponse, E>>,
    E: Into<BoxError>,
{
    type Output = Result<HttpResponse, BoxError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let result_fut = this.response_future.poll(cx);

        match result_fut {
            Poll::Ready(result) => match result {
                Ok(response) => Poll::Ready(try_map_http_response(response).map_err(Into::into)),
                Err(e) => Poll::Ready(Err(e.into())),
            },
            Poll::Pending => Poll::Pending,
        }
    }
}

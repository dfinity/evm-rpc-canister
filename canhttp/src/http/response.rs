use crate::convert::Convert;
use ic_cdk::api::management_canister::http_request::HttpResponse as IcHttpResponse;
use pin_project::pin_project;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use thiserror::Error;
use tower::{BoxError, Service};
use tower_layer::Layer;

/// HTTP response with a body made of bytes.
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

#[derive(Debug, Clone)]
pub struct HttpResponseConverter;

impl Convert<IcHttpResponse> for HttpResponseConverter {
    type Output = HttpResponse;
    type Error = HttpResponseConversionError;

    fn try_convert(&mut self, response: IcHttpResponse) -> Result<Self::Output, Self::Error> {
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
}

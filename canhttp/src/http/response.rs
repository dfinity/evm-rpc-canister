use crate::convert::Convert;
use http::Response;
use ic_cdk::api::management_canister::http_request::HttpResponse as IcHttpResponse;
use thiserror::Error;

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

#[derive(Error, Clone, Debug)]
pub enum FilterNonSuccessulHttpResponseError<T> {
    #[error("HTTP response is not successful: {0:?}")]
    UnsuccessfulResponse(http::Response<T>),
}

#[derive(Clone, Debug)]
pub struct FilterNonSuccessfulHttpResponse;

impl<T> Convert<http::Response<T>> for FilterNonSuccessfulHttpResponse {
    type Output = http::Response<T>;
    type Error = FilterNonSuccessulHttpResponseError<T>;

    fn try_convert(&mut self, response: Response<T>) -> Result<Self::Output, Self::Error> {
        if !response.status().is_success() {
            return Err(FilterNonSuccessulHttpResponseError::UnsuccessfulResponse(
                response,
            ));
        }
        Ok(response)
    }
}

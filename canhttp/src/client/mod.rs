#[cfg(test)]
mod tests;

use crate::convert::ConvertError;
use crate::ConvertServiceBuilder;
use ic_cdk::api::call::RejectionCode;
use ic_cdk::api::management_canister::http_request::{
    CanisterHttpRequestArgument as IcHttpRequest, HttpResponse as IcHttpResponse, TransformContext,
};
use ic_error_types::RejectCode;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use thiserror::Error;
use tower::{BoxError, Service, ServiceBuilder};

/// Thin wrapper around [`ic_cdk::api::management_canister::http_request::http_request`]
/// that implements the [`tower::Service`] trait. Its functionality can be extended by composing so-called
/// [tower middlewares](https://docs.rs/tower/latest/tower/#usage).
///
/// Middlewares from this crate:
/// * [`crate::cycles::CyclesAccounting`]: handles cycles accounting.
/// * [`crate::observability`]: add logging or metrics.
/// * [`crate::http`]: use types from the [http](https://crates.io/crates/http) crate for requests and responses.
/// * [`crate::retry::DoubleMaxResponseBytes`]: automatically retry failed requests due to the response being too big.
#[derive(Clone, Debug)]
pub struct Client;

impl Client {
    /// Create a new client returning custom errors.
    pub fn new_with_error<CustomError: From<IcError>>() -> ConvertError<Client, CustomError> {
        ServiceBuilder::new()
            .convert_error::<CustomError>()
            .service(Client)
    }

    /// Creates a new client where error type is erased.
    pub fn new_with_box_error() -> ConvertError<Client, BoxError> {
        Self::new_with_error::<BoxError>()
    }
}

/// Error returned by the Internet Computer when making an HTTPs outcall.
#[derive(Error, Clone, Debug, PartialEq, Eq)]
#[error("Error from ICP: (code {code:?}, message {message})")]
pub struct IcError {
    /// Rejection code as specified [here](https://internetcomputer.org/docs/current/references/ic-interface-spec#reject-codes)
    pub code: RejectCode,
    /// Associated helper message.
    pub message: String,
}

impl Service<IcHttpRequestWithCycles> for Client {
    type Response = IcHttpResponse;
    type Error = IcError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(
        &mut self,
        IcHttpRequestWithCycles { request, cycles }: IcHttpRequestWithCycles,
    ) -> Self::Future {
        use ic_cdk::api::call::RejectionCode as IcCdkRejectionCode;
        fn convert_reject_code(code: IcCdkRejectionCode) -> RejectCode {
            match code {
                IcCdkRejectionCode::SysFatal => RejectCode::SysFatal,
                IcCdkRejectionCode::SysTransient => RejectCode::SysTransient,
                IcCdkRejectionCode::DestinationInvalid => RejectCode::DestinationInvalid,
                IcCdkRejectionCode::CanisterReject => RejectCode::CanisterReject,
                IcCdkRejectionCode::CanisterError => RejectCode::CanisterError,
                IcCdkRejectionCode::Unknown => {
                    // This can only happen if there is a new error code on ICP that the CDK is not aware of.
                    // We map it to SysFatal since none of the other error codes apply.
                    // In particular, note that RejectCode::SysUnknown is only applicable to inter-canister calls that used ic0.call_with_best_effort_response.
                    RejectCode::SysFatal
                }
                RejectionCode::NoError => unreachable!("ic_cdk::api::management_canister::http_request::http_request should never produce a RejectionCode::NoError error")
            }
        }

        Box::pin(async move {
            match ic_cdk::api::management_canister::http_request::http_request(request, cycles)
                .await
            {
                Ok((response,)) => Ok(response),
                Err((code, message)) => Err(IcError {
                    code: convert_reject_code(code),
                    message,
                }),
            }
        })
    }
}

/// [`IcHttpRequest`] specifying how many cycles should be attached for the HTTPs outcall.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IcHttpRequestWithCycles {
    /// Request to be made.
    pub request: IcHttpRequest,
    /// Number of cycles to attach.
    pub cycles: u128,
}

/// Add support for max response bytes.
pub trait MaxResponseBytesRequestExtension: Sized {
    /// Set the max response bytes.
    ///
    /// If provided, the value must not exceed 2MB (2_000_000B).
    /// The call will be charged based on this parameter.
    /// If not provided, the maximum of 2MB will be used.
    fn set_max_response_bytes(&mut self, value: u64);

    /// Retrieves the current max response bytes value, if any.
    fn get_max_response_bytes(&self) -> Option<u64>;

    /// Convenience method to use the builder pattern.
    fn max_response_bytes(mut self, value: u64) -> Self {
        self.set_max_response_bytes(value);
        self
    }
}

impl MaxResponseBytesRequestExtension for IcHttpRequest {
    fn set_max_response_bytes(&mut self, value: u64) {
        self.max_response_bytes = Some(value);
    }

    fn get_max_response_bytes(&self) -> Option<u64> {
        self.max_response_bytes
    }
}

impl MaxResponseBytesRequestExtension for IcHttpRequestWithCycles {
    fn set_max_response_bytes(&mut self, value: u64) {
        self.request.set_max_response_bytes(value);
    }

    fn get_max_response_bytes(&self) -> Option<u64> {
        self.request.get_max_response_bytes()
    }
}

/// Add support for transform context to specify how the response will be canonicalized by the replica
/// to maximize chances of consensus.
///
/// See the [docs](https://internetcomputer.org/docs/references/https-outcalls-how-it-works#transformation-function)
/// on HTTPs outcalls for more details.
pub trait TransformContextRequestExtension: Sized {
    /// Set the transform context.
    fn set_transform_context(&mut self, value: TransformContext);

    /// Retrieve the current transform context, if any.
    fn get_transform_context(&self) -> Option<&TransformContext>;

    /// Convenience method to use the builder pattern.
    fn transform_context(mut self, value: TransformContext) -> Self {
        self.set_transform_context(value);
        self
    }
}

impl TransformContextRequestExtension for IcHttpRequest {
    fn set_transform_context(&mut self, value: TransformContext) {
        self.transform = Some(value);
    }

    fn get_transform_context(&self) -> Option<&TransformContext> {
        self.transform.as_ref()
    }
}

impl TransformContextRequestExtension for IcHttpRequestWithCycles {
    fn set_transform_context(&mut self, value: TransformContext) {
        self.request.set_transform_context(value);
    }

    fn get_transform_context(&self) -> Option<&TransformContext> {
        self.request.get_transform_context()
    }
}

/// Characterize errors that are specific to HTTPs outcalls.
pub trait HttpsOutcallError {
    /// Determines whether the error indicates that the response was larger than the specified
    /// [`max_response_bytes`](https://internetcomputer.org/docs/current/references/ic-interface-spec#ic-http_request) specified in the request.
    ///
    /// If true, retrying with a larger value for `max_response_bytes` may help.
    fn is_response_too_large(&self) -> bool;
}

impl HttpsOutcallError for IcError {
    fn is_response_too_large(&self) -> bool {
        self.code == RejectCode::SysFatal
            && (self.message.contains("size limit") || self.message.contains("length limit"))
    }
}

impl HttpsOutcallError for BoxError {
    fn is_response_too_large(&self) -> bool {
        if let Some(ic_error) = self.downcast_ref::<IcError>() {
            return ic_error.is_response_too_large();
        }
        false
    }
}

use crate::IcError;
use ic_cdk::api::management_canister::http_request::CanisterHttpRequestArgument;
use std::future;
use tower::retry;

/// Double the `max_response_bytes` in case the IC error indicates the response was too big.
#[derive(Debug, Clone)]
pub struct DoubleMaxResponseBytes;

impl<Response> retry::Policy<CanisterHttpRequestArgument, Response, IcError>
    for DoubleMaxResponseBytes
{
    type Future = future::Ready<()>;

    fn retry(
        &mut self,
        req: &mut CanisterHttpRequestArgument,
        result: &mut Result<Response, IcError>,
    ) -> Option<Self::Future> {
        // This constant comes from the IC specification:
        // > If provided, the value must not exceed 2MB
        const HTTP_MAX_SIZE: u64 = 2_000_000;

        match result {
            Ok(_) => None,
            Err(e) => {
                if e.is_response_too_large() {
                    if let Some(previous_estimate) = req.max_response_bytes {
                        let new_estimate = previous_estimate
                            .max(1024)
                            .saturating_mul(2)
                            .min(HTTP_MAX_SIZE);
                        if new_estimate > previous_estimate {
                            req.max_response_bytes = Some(new_estimate);
                            return Some(future::ready(()));
                        }
                    } // no estimate means the maximum was already used
                }
                None
            }
        }
    }

    fn clone_request(
        &mut self,
        req: &CanisterHttpRequestArgument,
    ) -> Option<CanisterHttpRequestArgument> {
        Some(req.clone())
    }
}

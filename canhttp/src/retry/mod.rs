use crate::{HttpsOutcallError, MaxResponseBytesRequestExtension};
use std::future;
use tower::retry;

// This constant comes from the IC specification:
// > If provided, the value must not exceed 2MB
const HTTP_MAX_SIZE: u64 = 2_000_000;

/// Double the `max_response_bytes` in case the IC error indicates the response was too big.
#[derive(Debug, Clone)]
pub struct DoubleMaxResponseBytes;

impl<Request, Response, Error> retry::Policy<Request, Response, Error> for DoubleMaxResponseBytes
where
    Request: MaxResponseBytesRequestExtension + Clone,
    Error: HttpsOutcallError,
{
    type Future = future::Ready<()>;

    fn retry(
        &mut self,
        req: &mut Request,
        result: &mut Result<Response, Error>,
    ) -> Option<Self::Future> {
        match result {
            Err(e) if e.is_response_too_large() => {
                if let Some(previous_estimate) = req.get_max_response_bytes() {
                    let new_estimate = previous_estimate
                        .max(1024)
                        .saturating_mul(2)
                        .min(HTTP_MAX_SIZE);
                    if new_estimate > previous_estimate {
                        req.set_max_response_bytes(new_estimate);
                        return Some(future::ready(()));
                    }
                }
                None
            }
            _ => None,
        }
    }

    fn clone_request(&mut self, req: &Request) -> Option<Request> {
        match req.get_max_response_bytes() {
            Some(max_response_bytes) if max_response_bytes < HTTP_MAX_SIZE => Some(req.clone()),
            // Not having a value is equivalent to setting `max_response_bytes` to the maximum value.
            // If there is a value, it's at least the maximum value.
            // In both cases retrying will not help.
            _ => None,
        }
    }
}

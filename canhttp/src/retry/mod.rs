use crate::request::MaxResponseBytesRequestExtension;
use crate::IcError;
use std::future;
use tower::retry;

/// Double the `max_response_bytes` in case the IC error indicates the response was too big.
#[derive(Debug, Clone)]
pub struct DoubleMaxResponseBytes;

impl<Request, Response> retry::Policy<Request, Response, IcError> for DoubleMaxResponseBytes
where
    Request: MaxResponseBytesRequestExtension + Clone,
{
    type Future = future::Ready<()>;

    fn retry(
        &mut self,
        req: &mut Request,
        result: &mut Result<Response, IcError>,
    ) -> Option<Self::Future> {
        // This constant comes from the IC specification:
        // > If provided, the value must not exceed 2MB
        const HTTP_MAX_SIZE: u64 = 2_000_000;

        match result {
            Ok(_) => None,
            Err(ic_error) => {
                if ic_error.is_response_too_large() {
                    if let Some(previous_estimate) = req.get_max_response_bytes() {
                        let new_estimate = previous_estimate
                            .max(1024)
                            .saturating_mul(2)
                            .min(HTTP_MAX_SIZE);
                        if new_estimate > previous_estimate {
                            req.set_max_response_bytes(new_estimate);
                            return Some(future::ready(()));
                        }
                    } // no estimate means the maximum was already used
                }
                None
            }
        }
    }

    fn clone_request(&mut self, req: &Request) -> Option<Request> {
        Some(req.clone())
    }
}

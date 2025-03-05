//! Library to make [HTTPs outcalls](https://internetcomputer.org/https-outcalls)
//! from a canister on the Internet Computer,
//! leveraging the modularity of the [tower framework](https://rust-lang.guide/guide/learn-async-rust/tower.html).

#![forbid(unsafe_code)]
#![forbid(missing_docs)]

pub use client::{Client, IcError, IcHttpRequestWithCycles};
pub use convert::ConvertServiceBuilder;
pub use cycles::{
    CyclesAccounting, CyclesAccountingError, CyclesChargingPolicy, CyclesCostEstimator,
};
use ic_cdk::api::management_canister::http_request::TransformContext;

mod client;
pub mod convert;
mod cycles;
#[cfg(feature = "http")]
pub mod http;
pub mod observability;

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
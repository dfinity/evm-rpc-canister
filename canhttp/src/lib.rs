//! Library to make [HTTPs outcalls](https://internetcomputer.org/https-outcalls)
//! from a canister on the Internet Computer,
//! leveraging the modularity of the [tower framework](https://rust-lang.guide/guide/learn-async-rust/tower.html).

#![forbid(unsafe_code)]
#![forbid(missing_docs)]

pub use client::{Client, IcError};
pub use cycles::{
    CyclesAccounting, CyclesAccountingError, DefaultRequestCyclesCostEstimator,
    EstimateRequestCyclesCost,
};
pub use request::{
    HttpRequest, HttpRequestFilter, MaxResponseBytesRequestExtensionBuilder,
    TransformContextRequestExtensionBuiler,
};
pub use response::{map_ic_http_response, FullBytes, HttpResponse};
pub use retry::DoubleMaxResponseBytes;

mod client;
mod cycles;
mod request;
mod response;
mod retry;

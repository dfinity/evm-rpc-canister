//! Library to make [HTTPs outcalls](https://internetcomputer.org/https-outcalls)
//! from a canister on the Internet Computer,
//! leveraging the modularity of the [tower framework](https://rust-lang.guide/guide/learn-async-rust/tower.html).

#![forbid(unsafe_code)]
// #![forbid(missing_docs)]

pub use client::{
    Client, HttpsOutcallError, IcError, IcHttpRequestWithCycles, MaxResponseBytesRequestExtension,
    TransformContextRequestExtension,
};
pub use convert::ConvertServiceBuilder;
pub use cycles::{
    CyclesAccounting, CyclesAccountingError, CyclesChargingPolicy, CyclesCostEstimator,
};
pub use multi::parallel_call;

mod client;
pub mod convert;
mod cycles;
#[cfg(feature = "http")]
pub mod http;
#[cfg(feature = "multi")]
mod multi;
pub mod observability;
pub mod retry;

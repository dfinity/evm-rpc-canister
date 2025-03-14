//! Library to make [HTTPs outcalls](https://internetcomputer.org/https-outcalls)
//! from a canister on the Internet Computer,
//! leveraging the modularity of the [tower framework](https://rust-lang.guide/guide/learn-async-rust/tower.html).

#![forbid(unsafe_code)]
#![forbid(missing_docs)]

pub use client::{
    Client, HttpsOutcallError, IcError, IcHttpRequestWithCycles, MaxResponseBytesRequestExtension,
    TransformContextRequestExtension,
};
pub use convert::ConvertServiceBuilder;
pub use cycles::{
    CyclesAccounting, CyclesAccountingError, CyclesChargingPolicy, CyclesCostEstimator,
};
mod client;
pub mod convert;
mod cycles;
#[cfg(feature = "http")]
pub mod http;
pub mod observability;
pub mod retry;

//! Library to make [HTTPs outcalls](https://internetcomputer.org/https-outcalls)
//! from a canister on the Internet Computer,
//! leveraging the modularity of the [tower framework](https://rust-lang.guide/guide/learn-async-rust/tower.html).

#![forbid(unsafe_code)]
// #![forbid(missing_docs)]

pub use client::{Client, IcError, IcHttpRequestWithCycles};
pub use cycles::{
    CyclesAccounting, CyclesAccountingError, CyclesChargingPolicy, CyclesCostEstimator,
};
pub use convert::ConvertResponseServiceBuilder;

mod client;
mod cycles;
mod convert;
#[cfg(feature = "http")]
pub mod http;
pub mod observability;

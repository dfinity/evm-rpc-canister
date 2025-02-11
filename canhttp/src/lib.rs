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
pub use retry::DoubleMaxResponseBytes;

mod client;
mod cycles;
mod retry;

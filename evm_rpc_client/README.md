[![Internet Computer portal](https://img.shields.io/badge/InternetComputer-grey?logo=internet%20computer&style=for-the-badge)](https://internetcomputer.org)
[![DFinity Forum](https://img.shields.io/badge/help-post%20on%20forum.dfinity.org-blue?style=for-the-badge)](https://forum.dfinity.org/t/sol-rpc-canister/41896)
[![GitHub license](https://img.shields.io/badge/license-Apache%202.0-blue.svg?logo=apache&style=for-the-badge)](LICENSE)

# Crate `evm_rpc_client`

Library to interact with the [EVM RPC canister](https://github.com/dfinity/evm-rpc-canister/) from a canister running on
the Internet Computer.
See the Rust [documentation](https://docs.rs/evm_rpc_client) for more details.

## Build Requirements

If you are using the `sol_rpc_types` crate inside a canister, make sure to follow these steps to ensure your code compiles:

**Override `getrandom` features**  
Add the following to your `Cargo.toml` file:
```toml
getrandom = { version = "*", features = ["custom"] }
```
This ensures that the `getrandom` crate (a transitive dependency of the Solana SDK) does not pull in `wasm-bindgen`, which is incompatible with canister environments.
> 💡 You can also specify an exact version of `getrandom`, as long as the `custom` feature is enabled, e.g. `getrandom = { version = "0.2.14", features = ["custom"] }`.

For more information, see [this blog post](https://forum.dfinity.org/t/module-imports-function-wbindgen-describe-from-wbindgen-placeholder-that-is-not-exported-by-the-runtime/11545/6).
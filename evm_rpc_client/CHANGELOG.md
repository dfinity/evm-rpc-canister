# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.4.0] - 2025-10-20

### Added

- Add methods to modify RPC config to `RequestBuilder` ([#494](https://github.com/dfinity/evm-rpc-canister/pull/494))
- Add `alloy` feature flag to `evm_rpc_client` ([#484](https://github.com/dfinity/evm-rpc-canister/pull/484))
- Add new `json_request` endpoint ([#477](https://github.com/dfinity/evm-rpc-canister/pull/477))
- Add client support for `eth_getTransactionReceipt` ([#476](https://github.com/dfinity/evm-rpc-canister/pull/476))
- Add `eth_sendRawTransaction` client support ([#467](https://github.com/dfinity/evm-rpc-canister/pull/467))
- Add client support for `eth_call` ([#466](https://github.com/dfinity/evm-rpc-canister/pull/466))
- Add client support for `eth_getTransactionCount` ([#465](https://github.com/dfinity/evm-rpc-canister/pull/465))
- Add support for `eth_feeHistory` to client ([#460](https://github.com/dfinity/evm-rpc-canister/pull/460))
- Add support for `eth_getBlockByNumber` to client ([#459](https://github.com/dfinity/evm-rpc-canister/pull/459))
- Add EVM RPC canister client ([#447](https://github.com/dfinity/evm-rpc-canister/pull/447))

### Changed

- Increase `IcError` granularity
- Merge branch 'main' into lpahlavi/XC-502-update-ic-cdk

### Fixed

- Client integration test infrastructure ([#448](https://github.com/dfinity/evm-rpc-canister/pull/448))


## [Unreleased]
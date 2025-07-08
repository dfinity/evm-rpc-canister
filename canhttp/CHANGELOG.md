# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2025-07-08

### Added
- Data structures `TimedSizedVec<T>` and `TimedSizedMap<K, V>` to store a limited number of expiring elements ([#434](https://github.com/dfinity/evm-rpc-canister/pull/434))
- Method to list `Ok` results in a `MultiResults` ([#435](https://github.com/dfinity/evm-rpc-canister/pull/435))

### Changed

- **Breaking:** change the `code` field in the `IcError` type to use `ic_error_types::RejectCode` instead of `ic_cdk::api::call::RejectionCode` ([#428](https://github.com/dfinity/evm-rpc-canister/pull/428))

## [0.1.0] - 2025-06-04

### Added

- JSON-RPC request ID with constant binary size ([#397](https://github.com/dfinity/evm-rpc-canister/pull/397))
- Use `canhttp` to make parallel calls ([#391](https://github.com/dfinity/evm-rpc-canister/pull/391))
- Improve validation of JSON-RPC requests and responses to adhere to the JSON-RPC specification ([#386](https://github.com/dfinity/evm-rpc-canister/pull/386) and [#387](https://github.com/dfinity/evm-rpc-canister/pull/387))
- Retry layer ([#378](https://github.com/dfinity/evm-rpc-canister/pull/378))
- JSON RPC conversion layer ([#375](https://github.com/dfinity/evm-rpc-canister/pull/375))
- HTTP conversion layer ([#374](https://github.com/dfinity/evm-rpc-canister/pull/374))
- Observability layer ([#370](https://github.com/dfinity/evm-rpc-canister/pull/370))
- Library `canhttp` ([#364](https://github.com/dfinity/evm-rpc-canister/pull/364))
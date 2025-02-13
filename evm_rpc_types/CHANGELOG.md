# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.3.0] - 2025-02-11

### Added

- The number of nodes in a subnet can be specified in the installation arguments (new optional field `nodes_in_subnet` in `InstallArgs`).
- Allow to override provider URL upon installation for testing purposes (new optional field `override_provider` in `InstallArgs`)

## [1.2.0] - 2024-10-17

### Added

- v1.2.0 Added types to support `eth_call.`

### Fixed

- Added missing `serde::Deserialize` annotation on `Provider`, `RpcAccess` and `RpcAuth` structs.

## [1.1.0] - 2024-10-14

### Changed

- v1.1.0 Improve Debug and Display implementations for `HexByte`, `Hex20`, `Hex32`, `Hex256`, `Hex` and `Nat256`.
- v1.1.0 Improve Debug implementation of `RpcApi`.

## [1.0.0] - 2024-10-07

### Added

- v1.0.0 Move `InstallArgs` and associated types to this crate.
- v1.0.0 Move `Provider` and associated types to this crate.
- v1.0.0 `Nat256`: transparent wrapper around a `Nat` to guarantee that it fits in 256 bits.
- v1.0.0 `HexByte`, `Hex20`, `Hex32`, `Hex256` and `Hex` : Candid types wrapping an amount of bytes (`u8` for `HexByte`,
  `[u8; N]` for `HexN`, and `Vec<u8>` for `Hex`) that can be represented as an hexadecimal string (prefixed by `0x`)
  when serialized.
- v1.0.0 Move `Block` to this crate.
- v1.0.0 Move `BlockTag` to this crate.
- v1.0.0 Move `FeeHistoryArgs` and `FeeHistory` to this crate.
- v1.0.0 Move `GetLogsArgs` and `LogEntry` to this crate.
- v1.0.0 Move `GetTransactionCountArgs` to this crate.
- v1.0.0 Move `RpcConfig` to this crate.
- v1.0.0 Move `SendRawTransactionStatus` to this crate.
- v1.0.0 Move `TransactionReceipt` to this crate.
- v1.0.0 Move providers-related types `EthMainnetService`, `EthSepoliaService`, `HttpHeader`, `L2MainnetService`,
  `RpcApi`, `RpcConfig`, `RpcService`, `RpcServices` to this crate.
- v1.0.0 Move result-related types `HttpOutcallError`, `JsonRpcError`, `MultiRpcResult`, `ProviderError`, `RpcError`,
  `RpcResult`, `ValidationError` to this crate.

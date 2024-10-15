# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

* v2.1.1 fix: ensure Candid API is the same as the interface exposed by the canister

## [2.1.0] - 2024-10-14

### Added

* v2.1.0 chore: changelog file

### Changed

* v2.1.0 refactor: Remove Ankr as a default provider 


## [2.0.0] - 2024-10-08

SHA-256 hash: `5c49768f03f075ffd87f0f20d849897e03db58d05fb0d12f1a340b2a5f1e4f65`

### Summary

This release brings the following main changes (see details below)

1. RPC providers are immutable (#244). The following [post](https://forum.dfinity.org/t/evm-rpc-canister/23313/53) explains in more detail the motivation behind this change.
2. Caller can now choose between equality or a threshold consensus strategy to aggregate responses from multiple providers (#284).
3. Optional canister logs (#201).
4. All (productive) dependencies on the IC repository were removed (mainly #243).


### Added

* build: set up reproducible builds via Docker by @rvanasa in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/206
* feat!: add collateral cycles by @rvanasa in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/213
* candid: L2 chains (Arbitrum, Base, Optimism) by @rvanasa in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/212
* feat: ability to update chain IDs via `manageProvider` method by @rvanasa in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/215
* dfx: add default canister init args by @rvanasa in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/220
* Add Llama RPC providers by @rvanasa in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/222
* feat: `evm_rpc_types` crate and `Nat256` by @gregorydemay in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/257
* feat!: NNS-managed RPC providers by @rvanasa in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/252
* feat: Choose a consensus strategy by @gregorydemay in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/286
* feat: implement threshold strategy by @gregorydemay in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/287
* feat: optional console log message filter by @rvanasa in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/241
* feat: implement `Error` trait for error types by @gregorydemay in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/298

### Changed

* Update README.md by @letmejustputthishere in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/203
* chore: update readme by @rvanasa in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/216
* Update README.md by @letmejustputthishere in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/219
* ci: mitigate rate limits / inconsistent responses by @rvanasa in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/227
* ci: bump `setup-protoc` action to v3 by @rvanasa in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/226
* auth: allow any principal to call `getAuthorized` by @rvanasa in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/225
* ci: skip flaky test by @rvanasa in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/230
* docs: minimal change to readme by @rvanasa in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/239
* chore: misc. by @rvanasa in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/247
* Minor typo in the README.md by @ChitranshVashney in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/260
* test: RPC provider API key modifications by @rvanasa in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/269
* Update README.md by @letmejustputthishere in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/280
* chore: bump dfx version by @rvanasa in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/221
* candid: clarify `topics` in `eth_getLogs` args by @rvanasa in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/197
* test: add example Candid-RPC method to Rust E2E test by @rvanasa in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/198
* rpc: update default Public Node JSON-RPC URLs by @rvanasa in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/199
* refactor: move `GetLogsArgs` and `LogEntry` to the `evm_rpc_types` crate by @gregorydemay in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/261
* refactor: move `TransactionReceipt` to `evm_rpc_types` by @gregorydemay in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/263
* refactor: move `Block` to `evm_rpc_types`  by @gregorydemay in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/268
* refactor: move `GetTransactionCountArgs` and `SendRawTransactionStatus` to `evm_rpc_types` by @gregorydemay in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/270
* refactor: move types related to providers to `evm-rpc-types` crate by @gregorydemay in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/271
* refactor: move types related to result to `evm-rpc-types` by @gregorydemay in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/274
* refactor: Remove dependency on `ic-cketh-minter` by @gregorydemay in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/275
* refactor: re-enable logs by @gregorydemay in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/276
* refactor: remove `cketh_eth_rpc_call_retry_count` metrics by @gregorydemay in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/277
* refactor: simplify `EthRpcClient` by @gregorydemay in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/278
* refactor: consolidate JSON requests and responses by @gregorydemay in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/279
* refactor: simplify `CheckedAmountOf` by @gregorydemay in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/283
* refactor: move types by @gregorydemay in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/285
* candid: rename `InitArgs` to `InstallArgs` by @rvanasa in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/272
* refactor: Use strongly typed fields in JSON requests and responses by @gregorydemay in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/291
* chore: Remove dependencies on the IC repository by @gregorydemay in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/293
* chore: upgrade all dependencies by @gregorydemay in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/294
* refactor: use `evm_rpc_types` in Rust E2E canister by @rvanasa in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/295
* refactor: Move `InstallArgs` and `Provider` to the `evm_rpc_types` crate by @gregorydemay in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/299
* build(deps): bump serde_json from 1.0.114 to 1.0.115 by @dependabot in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/188
* build(deps): bump serde from 1.0.198 to 1.0.202 by @dependabot in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/211
* build(deps): bump serde_json from 1.0.116 to 1.0.117 by @dependabot in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/209
* build(deps): bump num-traits from 0.2.18 to 0.2.19 by @dependabot in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/207
* build(deps): bump num from 0.4.2 to 0.4.3 by @dependabot in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/210
* build(deps): bump serde from 1.0.202 to 1.0.203 by @dependabot in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/217
* build(deps): bump url from 2.5.0 to 2.5.1 by @dependabot in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/223
* build(deps): bump url from 2.5.1 to 2.5.2 by @dependabot in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/228
* build(deps): bump serde_bytes from 0.11.14 to 0.11.15 by @dependabot in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/232
* build(deps): bump serde_json from 1.0.117 to 1.0.118 by @dependabot in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/231
* build(deps): bump serde_json from 1.0.118 to 1.0.120 by @dependabot in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/236
* build(deps): bump serde from 1.0.203 to 1.0.204 by @dependabot in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/238
* build(deps): bump serde_json from 1.0.120 to 1.0.121 by @dependabot in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/245
* build(deps): bump serde_json from 1.0.121 to 1.0.122 by @dependabot in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/246
* build(deps): bump async-trait from 0.1.80 to 0.1.81 by @dependabot in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/237
* build(deps): bump serde_json from 1.0.122 to 1.0.124 by @dependabot in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/250
* build(deps): bump serde from 1.0.204 to 1.0.206 by @dependabot in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/249
* build(deps): bump serde from 1.0.206 to 1.0.207 by @dependabot in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/251
* build(deps): bump ic-cdk from 0.10.0 to 0.10.1 by @dependabot in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/256
* build(deps): bump serde_json from 1.0.124 to 1.0.127 by @dependabot in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/259
* build(deps): bump serde from 1.0.207 to 1.0.209 by @dependabot in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/258
* build(deps): bump serde from 1.0.209 to 1.0.210 by @dependabot in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/267
* build(deps): bump serde_json from 1.0.127 to 1.0.128 by @dependabot in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/266
* build(deps): bump async-trait from 0.1.81 to 0.1.82 by @dependabot in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/265

### Fixed

* fix: dfx version in CI by @rvanasa in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/194
* fix: add `dfx pull` metadata to Docker build by @rvanasa in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/214
* fix: optional fields in `Block` by @gregorydemay in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/235
* fix!: simplify `eth_feeHistory` result type by @rvanasa in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/242
* fix!: change number of subnet nodes from 28 to 31 by @rvanasa in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/281
* fix!: change number of subnet nodes from 31 to 34 by @rvanasa in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/282
* fix: `TransactionReceipt` fields `status` and `to` should be optional by @gregorydemay in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/290
* fix: maximum value for `max_response_bytes` by @gregorydemay in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/297
* fix:  `repository` link in `evm_rpc_types/Cargo.toml` by @gregorydemay in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/301

### Removed

* candid: remove hostname from `UpdateProviderArgs` by @rvanasa in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/229
* candid!: remove `RpcService::Chain()` variant by @rvanasa in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/273


## New Contributors
* @letmejustputthishere made their first contribution in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/203
* @gregorydemay made their first contribution in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/235
* @ChitranshVashney made their first contribution in https://github.com/internet-computer-protocol/evm-rpc-canister/pull/260

**Full Changelog**: https://github.com/internet-computer-protocol/evm-rpc-canister/compare/release-2024-03-26...v2.0.0

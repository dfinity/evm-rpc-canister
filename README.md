# EVM RPC &nbsp;[![GitHub license](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](https://opensource.org/licenses/Apache-2.0) [![PRs Welcome](https://img.shields.io/badge/PRs-welcome-brightgreen.svg)](https://github.com/internet-computer-protocol/evm-rpc-canister/issues)

> #### Interact with [EVM blockchains](https://chainlist.org/?testnets=true) from the [Internet Computer](https://internetcomputer.org/).

## Overview

**EVM RPC** is an Internet Computer canister smart contract for communicating with [Ethereum](https://ethereum.org/en/) and other [EVM blockchains](https://chainlist.org/?testnets=true) using an on-chain API. 

This canister facilitates API requests to JSON-RPC services such as [CloudFlare](https://www.cloudflare.com/en-gb/web3/), [Alchemy](https://www.alchemy.com/), [Ankr](https://www.ankr.com/), or [BlockPI](https://blockpi.io/) using [HTTPS outcalls](https://internetcomputer.org/https-outcalls). This enables functionality similar to traditional Ethereum dapps, including querying Ethereum smart contract states and submitting raw transactions.

Beyond the Ethereum blockchain, this canister also has partial support for Polygon, Avalanche, and other popular EVM networks. Check out [ChainList.org](https://chainlist.org/?testnets=true) for an extensive list of networks and RPC providers.

You can read more about the inner workings of the EVM RPC canister [here](https://medium.com/dfinity/icp-ethereum-how-icps-evm-rpc-canister-connects-the-networks-b57909efecf6).

## Documentation

You can find extensive documentation for the EVM RPC canister in the [ICP developer docs](https://internetcomputer.org/docs/current/developer-docs/multi-chain/ethereum/evm-rpc/overview).

## Canister

The EVM RPC canister runs on the [fiduciary subnet](https://internetcomputer.org/docs/current/concepts/subnet-types#fiduciary-subnets) with the following principal: [`7hfb6-caaaa-aaaar-qadga-cai`](https://dashboard.internetcomputer.org/canister/7hfb6-caaaa-aaaar-qadga-cai). 

Refer to the [Reproducible Builds](#reproducible-builds) section for information on how to verify the hash of the deployed WebAssembly module.

## Quick Start

Add the following to your `icp.yaml` config file:

```yaml
canisters:
  - name: evm_rpc
    init_args: "(record {})"
    build:
      steps:
        - type: pre-built
          url: https://github.com/internet-computer-protocol/evm-rpc-canister/releases/latest/download/evm_rpc.wasm.gz
```

Run the following commands to deploy the canister in your local environment:

```sh
# Start the local network
icp network start -d

# Locally deploy the `evm_rpc` canister
icp deploy evm_rpc
```

To call the canister already deployed on the IC mainnet, use the principal `7hfb6-caaaa-aaaar-qadga-cai` directly — no local deployment required.

## Examples

### JSON-RPC (IC mainnet)

```bash
icp canister call 7hfb6-caaaa-aaaar-qadga-cai request '(variant {Chain=0x1},"{\"jsonrpc\":\"2.0\",\"method\":\"eth_gasPrice\",\"params\":[],\"id\":1}",1000)' --cycles 1000000000 --network ic
```

### JSON-RPC (local network)

Managed local networks include a proxy canister that forwards calls with cycles attached. Retrieve its principal from `icp network status`:

```bash
PROXY=$(icp network status --json | jq -r .proxy_canister_principal)

# Use a custom provider
icp canister call evm_rpc request '(variant {Custom=record {url="https://cloudflare-eth.com"}},"{\"jsonrpc\":\"2.0\",\"method\":\"eth_gasPrice\",\"params\":[],\"id\":1}",1000)' --proxy "$PROXY" --cycles 1000000000
icp canister call evm_rpc request '(variant {Custom=record {url="https://ethereum.publicnode.com"}},"{\"jsonrpc\":\"2.0\",\"method\":\"eth_gasPrice\",\"params\":[],\"id\":1}",1000)' --proxy "$PROXY" --cycles 1000000000

# Use a specific EVM chain
icp canister call evm_rpc request '(variant {Chain=0x1},"{\"jsonrpc\":\"2.0\",\"method\":\"eth_gasPrice\",\"params\":[],\"id\":1}",1000)' --proxy "$PROXY" --cycles 1000000000
```

## Reproducible Builds

The EVM RPC canister supports [reproducible builds](https://internetcomputer.org/docs/current/developer-docs/smart-contracts/test/reproducible-builds):

1. Ensure [Docker](https://www.docker.com/get-started/) is installed on your machine.
2. Run `scripts/docker-build` in your terminal. 
4. Run `sha256sum evm_rpc.wasm.gz` on the generated file to view the SHA-256 hash.

In order to verify the latest EVM RPC Wasm file, please make sure to download the corresponding version of the source code from the latest GitHub release.

## Contributing

Contributions are welcome! Please check out the [contributor guidelines](https://github.com/internet-computer-protocol/evm-rpc-canister/blob/main/.github/CONTRIBUTING.md) for more information.

Run the following commands to set up a local development environment:

```bash
# Clone the repository and install dependencies
git clone https://github.com/internet-computer-protocol/evm-rpc-canister
cd evm-rpc-canister

# This repo requires Node 24+ and uses pnpm. Either enable Corepack (bundled with Node):
corepack enable && corepack prepare pnpm@10.9.0 --activate
# ...or install pnpm directly:
#   npm install -g pnpm@10.9.0
pnpm install

# `icp`, `ic-wasm`, and `mops` are installed as versioned devDependencies.
# Put them on PATH, or prefix invocations with `pnpm exec`.
export PATH="$PWD/node_modules/.bin:$PATH"

# Deploy to the local network
icp network start -d
pnpm generate
icp deploy evm_rpc

# Alternatively, deploy and run test suite
icp network start -d
scripts/e2e
```

`scripts/e2e` and `pnpm generate:declarations` also require [`didc`](https://github.com/dfinity/candid/releases) on PATH.

Regenerate language bindings with the `generate` [pnpm script](https://pnpm.io/cli/run):

```bash
pnpm generate
```

## Learn More

* [Candid interface](https://github.com/internet-computer-protocol/evm-rpc-canister/blob/main/candid/evm_rpc.did)

## Related Projects

* [`evm-rpc-canister-types`](https://crates.io/crates/evm-rpc-canister-types/3.0.0): Rust types for interacting with the EVM RPC canister.
* [`ic-evm-utils`](https://crates.io/crates/ic-evm-utils): A convenience crate for interacting with the EVM RPC Canister from canisters written in Rust.
* [chain-fusion-starter](https://github.com/letmejustputthishere/chain-fusion-starter): starter template leveraging chain fusion technology to build EVM coprocessors on the Internet Computer Protocol.
* [Bitcoin canister](https://github.com/dfinity/bitcoin-canister): interact with the Bitcoin blockchain from the Internet Computer.
* [ckETH](https://forum.dfinity.org/t/cketh-a-canister-issued-ether-twin-token-on-the-ic/22819): a canister-issued Ether twin token on the Internet Computer.
* [ICP 🔗 ETH](https://github.com/dfinity/icp-eth-starter): a full-stack starter project for calling Ethereum smart contracts from an IC dapp.

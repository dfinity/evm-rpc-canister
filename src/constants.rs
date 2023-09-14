pub const INGRESS_OVERHEAD_BYTES: u128 = 100;
pub const INGRESS_MESSAGE_RECEIVED_COST: u128 = 1_200_000;
pub const INGRESS_MESSAGE_BYTE_RECEIVED_COST: u128 = 2_000;
pub const HTTP_OUTCALL_REQUEST_COST: u128 = 400_000_000;
pub const HTTP_OUTCALL_BYTE_RECEIEVED_COST: u128 = 100_000;
pub const BASE_SUBNET_SIZE: u128 = 13; // App subnet

pub const MINIMUM_WITHDRAWAL_CYCLES: u128 = 1_000_000_000;

pub const STRING_STORABLE_MAX_SIZE: u32 = 100;
pub const WASM_PAGE_SIZE: u64 = 65536;

pub const INITIAL_SERVICE_HOSTS_ALLOWLIST: &[&str] = &[
    "cloudflare-eth.com",
    "ethereum.publicnode.com",
    "eth-mainnet.g.alchemy.com",
    "eth-goerli.g.alchemy.com",
    "rpc.flashbots.net",
    "eth-mainnet.blastapi.io",
    "ethereumnodelight.app.runonflux.io",
    "eth.nownodes.io",
    "rpc.ankr.com",
    "mainnet.infura.io",
    "eth.getblock.io",
    "rpc.kriptonio.com",
    "api.0x.org",
    "erigon-mainnet--rpc.datahub.figment.io",
    "archivenode.io",
    "eth-mainnet.nodereal.io",
    "ethereum-mainnet.s.chainbase.online",
    "eth.llamarpc.com",
    "ethereum-mainnet-rpc.allthatnode.com",
    "api.zmok.io",
    "in-light.eth.linkpool.iono",
    "api.mycryptoapi.com",
    "mainnet.eth.cloud.ava.dono",
    "eth-mainnet.gateway.pokt.network",
];

// Static permissions. The canister creator is also authorized for all permissions.

// Principals allowed to send JSON RPCs.
pub const DEFAULT_NODES_IN_SUBNET: u32 = 13;
pub const DEFAULT_OPEN_RPC_ACCESS: bool = true;
pub const RPC_ALLOWLIST: &[&str] = &[];
// Principals allowed to registry API keys.
pub const REGISTER_PROVIDER_ALLOWLIST: &[&str] = &[];
// Principals that will not be charged cycles to send JSON RPCs.
pub const FREE_RPC_ALLOWLIST: &[&str] = &[];
// Principals who have Admin authorization.
pub const AUTHORIZED_ADMIN: &[&str] = &[];

use crate::*;

pub const ANKR_HOSTNAME: &str = "rpc.ankr.com";
pub const ALCHEMY_ETH_MAINNET_HOSTNAME: &str = "eth-mainnet.g.alchemy.com";
pub const ALCHEMY_ETH_SEPOLIA_HOSTNAME: &str = "eth-sepolia.g.alchemy.com";
pub const CLOUDFLARE_HOSTNAME: &str = "cloudflare-eth.com";
pub const BLOCKPI_ETH_MAINNET_HOSTNAME: &str = "ethereum.blockpi.network";
pub const BLOCKPI_ETH_SEPOLIA_HOSTNAME: &str = "ethereum-sepolia.blockpi.network";
pub const PUBLICNODE_ETH_MAINNET_HOSTNAME: &str = "ethereum.publicnode.com";
pub const PUBLICNODE_ETH_SEPOLIA_HOSTNAME: &str = "ethereum-sepolia.publicnode.com";
pub const ETH_SEPOLIA_HOSTNAME: &str = "rpc.sepolia.org";

// Limited API credentials for local testing.
// Use `dfx canister call evm_rpc updateProvider ...` to pass your own keys.
pub const ALCHEMY_ETH_MAINNET_CREDENTIAL: &str = "/v2/zBxaSBUMfuH8XnA-uLIWeXfCx1T8ItkM";
pub const ALCHEMY_ETH_SEPOLIA_CREDENTIAL: &str = "/v2/Mbow19DWsfPXiTpdgvRu4HQq63iYycU-";

pub fn get_default_providers() -> Vec<RegisterProviderArgs> {
    vec![
        RegisterProviderArgs {
            chain_id: ETH_MAINNET_CHAIN_ID,
            hostname: CLOUDFLARE_HOSTNAME.to_string(),
            credential_path: "/v1/mainnet".to_string(),
            credential_headers: None,
            cycles_per_call: 0,
            cycles_per_message_byte: 0,
        },
        RegisterProviderArgs {
            chain_id: ETH_MAINNET_CHAIN_ID,
            hostname: ANKR_HOSTNAME.to_string(),
            credential_path: "/eth".to_string(),
            credential_headers: None,
            cycles_per_call: 0,
            cycles_per_message_byte: 0,
        },
        RegisterProviderArgs {
            chain_id: ETH_MAINNET_CHAIN_ID,
            hostname: PUBLICNODE_ETH_MAINNET_HOSTNAME.to_string(),
            credential_path: "".to_string(),
            credential_headers: None,
            cycles_per_call: 0,
            cycles_per_message_byte: 0,
        },
        RegisterProviderArgs {
            chain_id: ETH_MAINNET_CHAIN_ID,
            hostname: BLOCKPI_ETH_MAINNET_HOSTNAME.to_string(),
            credential_path: "/v1/rpc/public".to_string(),
            credential_headers: None,
            cycles_per_call: 0,
            cycles_per_message_byte: 0,
        },
        RegisterProviderArgs {
            chain_id: ETH_SEPOLIA_CHAIN_ID,
            hostname: ETH_SEPOLIA_HOSTNAME.to_string(),
            credential_path: "".to_string(),
            credential_headers: None,
            cycles_per_call: 0,
            cycles_per_message_byte: 0,
        },
        RegisterProviderArgs {
            chain_id: ETH_SEPOLIA_CHAIN_ID,
            hostname: ANKR_HOSTNAME.to_string(),
            credential_path: "/eth_sepolia".to_string(),
            credential_headers: None,
            cycles_per_call: 0,
            cycles_per_message_byte: 0,
        },
        RegisterProviderArgs {
            chain_id: ETH_SEPOLIA_CHAIN_ID,
            hostname: BLOCKPI_ETH_SEPOLIA_HOSTNAME.to_string(),
            credential_path: "/v1/rpc/public".to_string(),
            credential_headers: None,
            cycles_per_call: 0,
            cycles_per_message_byte: 0,
        },
        RegisterProviderArgs {
            chain_id: ETH_SEPOLIA_CHAIN_ID,
            hostname: PUBLICNODE_ETH_SEPOLIA_HOSTNAME.to_string(),
            credential_path: "".to_string(),
            credential_headers: None,
            cycles_per_call: 0,
            cycles_per_message_byte: 0,
        },
        RegisterProviderArgs {
            chain_id: ETH_MAINNET_CHAIN_ID,
            hostname: ALCHEMY_ETH_MAINNET_HOSTNAME.to_string(),
            credential_path: ALCHEMY_ETH_MAINNET_CREDENTIAL.to_string(),
            credential_headers: None,
            cycles_per_call: 0,
            cycles_per_message_byte: 0,
        },
        RegisterProviderArgs {
            chain_id: ETH_SEPOLIA_CHAIN_ID,
            hostname: ALCHEMY_ETH_SEPOLIA_HOSTNAME.to_string(),
            credential_path: ALCHEMY_ETH_SEPOLIA_CREDENTIAL.to_string(),
            credential_headers: None,
            cycles_per_call: 0,
            cycles_per_message_byte: 0,
        },
    ]
}

pub fn find_provider(f: impl Fn(&Provider) -> bool) -> Option<Provider> {
    PROVIDERS.with(|providers| {
        let providers = providers.borrow();
        Some(
            providers
                .iter()
                .find(|(_, p)| p.primary && f(p))
                .or_else(|| providers.iter().find(|(_, p)| f(p)))?
                .1,
        )
    })
}

pub fn do_register_provider(caller: Principal, provider: RegisterProviderArgs) -> u64 {
    validate_hostname(&provider.hostname).unwrap();
    validate_credential_path(&provider.credential_path).unwrap();
    let provider_id = METADATA.with(|m| {
        let mut metadata = m.borrow().get().clone();
        let id = metadata.next_provider_id;
        metadata.next_provider_id += 1;
        m.borrow_mut().set(metadata).unwrap();
        id
    });
    PROVIDERS.with(|p| {
        p.borrow_mut().insert(
            provider_id,
            Provider {
                provider_id,
                owner: caller,
                chain_id: provider.chain_id,
                hostname: provider.hostname,
                credential_path: provider.credential_path,
                credential_headers: provider.credential_headers.unwrap_or_default(),
                cycles_per_call: provider.cycles_per_call,
                cycles_per_message_byte: provider.cycles_per_message_byte,
                cycles_owed: 0,
                primary: false,
            },
        )
    });
    provider_id
}

pub fn do_unregister_provider(caller: Principal, provider_id: u64) -> bool {
    PROVIDERS.with(|p| {
        let mut p = p.borrow_mut();
        if let Some(provider) = p.get(&provider_id) {
            if provider.owner == caller || is_authorized(&caller, Auth::Manage) {
                p.remove(&provider_id).is_some()
            } else {
                ic_cdk::trap("Not authorized");
            }
        } else {
            false
        }
    })
}

pub fn do_update_provider(caller: Principal, update: UpdateProviderArgs) {
    PROVIDERS.with(|p| {
        let mut p = p.borrow_mut();
        match p.get(&update.provider_id) {
            Some(mut provider) => {
                if provider.owner != caller && !is_authorized(&caller, Auth::Manage) {
                    ic_cdk::trap("Provider owner != caller");
                }
                if let Some(hostname) = update.hostname {
                    validate_hostname(&hostname).unwrap();
                    provider.hostname = hostname;
                }
                if let Some(path) = update.credential_path {
                    validate_credential_path(&path).unwrap();
                    provider.credential_path = path;
                }
                if let Some(headers) = update.credential_headers {
                    validate_credential_headers(&headers).unwrap();
                    provider.credential_headers = headers;
                }
                if let Some(primary) = update.primary {
                    provider.primary = primary;
                }
                if let Some(cycles_per_call) = update.cycles_per_call {
                    provider.cycles_per_call = cycles_per_call;
                }
                if let Some(cycles_per_message_byte) = update.cycles_per_message_byte {
                    provider.cycles_per_message_byte = cycles_per_message_byte;
                }
                p.insert(update.provider_id, provider);
            }
            None => ic_cdk::trap("Provider not found"),
        }
    });
}

mod static_map {
    use crate::providers::{PROVIDERS, SERVICE_PROVIDER_MAP};
    use std::collections::{BTreeSet, HashMap};

    use crate::{
        constants::API_KEY_REPLACE_STRING,
        types::{Provider, RpcAccess, RpcAuth},
    };

    #[test]
    fn test_provider_id_sequence() {
        for (i, provider) in PROVIDERS.iter().enumerate() {
            assert_eq!(provider.provider_id, i as u64);
        }
    }

    #[test]
    fn test_rpc_provider_url_patterns() {
        for provider in PROVIDERS {
            fn assert_not_url_pattern(url: &str, provider: &Provider) {
                assert!(
                    !url.contains(API_KEY_REPLACE_STRING),
                    "Unexpected API key in URL for provider: {}",
                    provider.provider_id
                )
            }
            fn assert_url_pattern(url: &str, provider: &Provider) {
                assert!(
                    url.contains(API_KEY_REPLACE_STRING),
                    "Missing API key in URL pattern for provider: {}",
                    provider.provider_id
                )
            }
            match &provider.access {
                RpcAccess::Authenticated { auth, public_url } => {
                    match auth {
                        RpcAuth::BearerToken { url } => assert_not_url_pattern(url, provider),
                        RpcAuth::UrlParameter { url_pattern } => {
                            assert_url_pattern(url_pattern, provider)
                        }
                    }
                    if let Some(public_url) = public_url {
                        assert_not_url_pattern(public_url, provider);
                    }
                }
                RpcAccess::Unauthenticated { public_url } => {
                    assert_not_url_pattern(public_url, provider);
                }
            }
        }
    }

    #[test]
    fn test_no_duplicate_service_providers() {
        SERVICE_PROVIDER_MAP.with(|map| {
            assert_eq!(
                map.len(),
                map.keys().collect::<BTreeSet<_>>().len(),
                "Duplicate service in mapping"
            );
            assert_eq!(
                map.len(),
                map.values().collect::<BTreeSet<_>>().len(),
                "Duplicate provider in mapping"
            );
        })
    }

    #[test]
    fn test_service_provider_coverage() {
        SERVICE_PROVIDER_MAP.with(|map| {
            let inverse_map: HashMap<_, _> = map.iter().map(|(k, v)| (v, k)).collect();
            for provider in PROVIDERS {
                assert!(
                    inverse_map.contains_key(&provider.provider_id),
                    "Missing service mapping for provider with ID: {}",
                    provider.provider_id
                );
            }
        })
    }
}

mod supported_rpc_service {
    use crate::providers::SupportedRpcService;
    use evm_rpc_types::{EthMainnetService, EthSepoliaService, L2MainnetService};
    use std::collections::BTreeSet;

    #[test]
    fn should_have_all_supported_providers() {
        fn assert_same_set(
            left: impl Iterator<Item = SupportedRpcService>,
            right: &[SupportedRpcService],
        ) {
            let left: BTreeSet<_> = left.collect();
            let right: BTreeSet<_> = right.iter().copied().collect();
            assert_eq!(left, right);
        }

        assert_same_set(
            EthMainnetService::all()
                .iter()
                .copied()
                .map(SupportedRpcService::EthMainnet),
            SupportedRpcService::eth_mainnet(),
        );

        assert_same_set(
            EthSepoliaService::all()
                .iter()
                .copied()
                .map(SupportedRpcService::EthSepolia),
            SupportedRpcService::eth_sepolia(),
        );

        assert_same_set(
            L2MainnetService::all()
                .iter()
                .copied()
                .map(SupportedRpcService::ArbitrumOne),
            SupportedRpcService::arbitrum_one(),
        );

        assert_same_set(
            L2MainnetService::all()
                .iter()
                .copied()
                .map(SupportedRpcService::BaseMainnet),
            SupportedRpcService::base_mainnet(),
        );

        assert_same_set(
            L2MainnetService::all()
                .iter()
                .copied()
                .map(SupportedRpcService::OptimismMainnet),
            SupportedRpcService::optimism_mainnet(),
        );
    }
}

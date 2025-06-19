use crate::providers::{PROVIDERS, SERVICE_PROVIDER_MAP};

mod static_map {
    use std::collections::{HashMap, HashSet};

    use crate::{
        constants::API_KEY_REPLACE_STRING,
        types::{Provider, RpcAccess, RpcAuth},
    };

    use super::{PROVIDERS, SERVICE_PROVIDER_MAP};

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
                map.keys().collect::<HashSet<_>>().len(),
                "Duplicate service in mapping"
            );
            assert_eq!(
                map.len(),
                map.values().collect::<HashSet<_>>().len(),
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

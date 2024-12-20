use super::{LogFilter, OverrideProvider, RegexString, RegexSubstitution};
use ic_stable_structures::Storable;
use proptest::prelude::{Just, Strategy};
use proptest::{option, prop_oneof, proptest};
use std::fmt::Debug;

proptest! {
    #[test]
    fn should_encode_decode_log_filter(value in arb_log_filter()) {
        test_encoding_decoding_roundtrip(&value);
    }

    #[test]
    fn should_encode_decode_override_provider(value in arb_override_provider()) {
        test_encoding_decoding_roundtrip(&value);
    }
}

fn arb_regex() -> impl Strategy<Value = RegexString> {
    ".*".prop_map(|r| RegexString::from(r.as_str()))
}

fn arb_regex_substitution() -> impl Strategy<Value = RegexSubstitution> {
    (arb_regex(), ".*").prop_map(|(pattern, replacement)| RegexSubstitution {
        pattern,
        replacement,
    })
}

fn arb_log_filter() -> impl Strategy<Value = LogFilter> {
    prop_oneof![
        Just(LogFilter::ShowAll),
        Just(LogFilter::HideAll),
        arb_regex().prop_map(LogFilter::ShowPattern),
        arb_regex().prop_map(LogFilter::HidePattern),
    ]
}

fn arb_override_provider() -> impl Strategy<Value = OverrideProvider> {
    option::of(arb_regex_substitution()).prop_map(|override_url| OverrideProvider { override_url })
}

fn test_encoding_decoding_roundtrip<T: Storable + PartialEq + Debug>(value: &T) {
    let bytes = value.to_bytes();
    let decoded_value = T::from_bytes(bytes);
    assert_eq!(value, &decoded_value);
}

mod override_provider {
    use crate::providers::PROVIDERS;
    use crate::types::{OverrideProvider, RegexSubstitution};
    use evm_rpc_types::RpcApi;
    use ic_cdk::api::management_canister::http_request::HttpHeader;

    #[test]
    fn should_override_provider_with_localhost() {
        let override_provider = override_to_localhost();
        for provider in PROVIDERS {
            let overriden_provider = override_provider.apply(provider.api());
            assert_eq!(
                overriden_provider,
                Ok(RpcApi {
                    url: "http://localhost:8545".to_string(),
                    headers: None
                })
            )
        }
    }

    #[test]
    fn should_be_noop_when_empty() {
        let no_override = OverrideProvider::default();
        for provider in PROVIDERS {
            let initial_api = provider.api();
            let overriden_api = no_override.apply(initial_api.clone());
            assert_eq!(Ok(initial_api), overriden_api);
        }
    }

    #[test]
    fn should_use_replacement_pattern() {
        let identity_override = OverrideProvider {
            override_url: Some(RegexSubstitution {
                pattern: "(?<url>.*)".into(),
                replacement: "$url".to_string(),
            }),
        };
        for provider in PROVIDERS {
            let initial_api = provider.api();
            let overriden_provider = identity_override.apply(initial_api.clone());
            assert_eq!(overriden_provider, Ok(initial_api))
        }
    }

    #[test]
    fn should_override_headers() {
        let identity_override = OverrideProvider {
            override_url: Some(RegexSubstitution {
                pattern: "(.*)".into(),
                replacement: "$1".to_string(),
            }),
        };
        for provider in PROVIDERS {
            let provider_with_headers = RpcApi {
                headers: Some(vec![HttpHeader {
                    name: "key".to_string(),
                    value: "123".to_string(),
                }]),
                ..provider.api()
            };
            let overriden_provider = identity_override.apply(provider_with_headers.clone());
            assert_eq!(
                overriden_provider,
                Ok(RpcApi {
                    url: provider_with_headers.url,
                    headers: None
                })
            )
        }
    }

    fn override_to_localhost() -> OverrideProvider {
        OverrideProvider {
            override_url: Some(RegexSubstitution {
                pattern: "^https://.*".into(),
                replacement: "http://localhost:8545".to_string(),
            }),
        }
    }
}

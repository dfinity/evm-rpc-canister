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

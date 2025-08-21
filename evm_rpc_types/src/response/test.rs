use crate::{Hex, Hex20, Hex32};
use proptest::prelude::Strategy;
use proptest::proptest;
use std::ops::RangeInclusive;

#[cfg(feature = "alloy")]
mod alloy_conversion_tests {
    use super::*;
    use crate::{LogEntry, Nat256};
    use num_bigint::BigUint;
    use proptest::arbitrary::any;
    use proptest::option;
    use serde_json::Value;
    use std::str::FromStr;

    proptest! {
        #[test]
        fn should_convert_from_alloy(entry in arb_log_entry()) {
            // Convert a number serialized as a hexadecimal string into an array of u32 digits
            fn hex_to_u32_digits(serialized: &mut Value, field: &str) {
                if let Some(Value::String(hex)) = serialized.get(field) {
                    let hex = hex.strip_prefix("0x").unwrap_or(hex);
                    let digits = BigUint::parse_bytes(hex.as_bytes(), 16).unwrap().to_u32_digits();
                    serialized[field] = digits.into();
                }
            }

            let serialized = serde_json::to_value(&entry).unwrap();

            let mut alloy_serialized = serde_json::to_value(&alloy_rpc_types::Log::try_from(entry.clone()).unwrap()).unwrap();
            hex_to_u32_digits(&mut alloy_serialized, "transactionIndex");
            hex_to_u32_digits(&mut alloy_serialized, "logIndex");
            hex_to_u32_digits(&mut alloy_serialized, "blockNumber");

            assert_eq!(serialized, alloy_serialized);
        }
    }

    fn arb_log_entry() -> impl Strategy<Value = LogEntry> {
        (
            arb_hex20(),
            arb_hex(),
            option::of(any::<u64>().prop_map(Nat256::from)),
            option::of(arb_hex32()),
            option::of(any::<u64>().prop_map(Nat256::from)),
            option::of(arb_hex32()),
            option::of(any::<u64>().prop_map(Nat256::from)),
            any::<bool>(),
        )
            .prop_map(
                |(
                    address,
                    data,
                    block_number,
                    transaction_hash,
                    transaction_index,
                    block_hash,
                    log_index,
                    removed,
                )| LogEntry {
                    address,
                    topics: vec![],
                    data,
                    block_number,
                    transaction_hash,
                    transaction_index,
                    block_hash,
                    log_index,
                    removed,
                },
            )
    }

    fn arb_hex20() -> impl Strategy<Value = Hex20> {
        arb_var_len_hex_string(20..=20_usize).prop_map(|s| Hex20::from_str(s.as_str()).unwrap())
    }

    fn arb_hex32() -> impl Strategy<Value = Hex32> {
        arb_var_len_hex_string(32..=32_usize).prop_map(|s| Hex32::from_str(s.as_str()).unwrap())
    }

    fn arb_hex() -> impl Strategy<Value = Hex> {
        arb_var_len_hex_string(0..=100_usize).prop_map(|s| Hex::from_str(s.as_str()).unwrap())
    }
}

fn arb_var_len_hex_string(num_bytes_range: RangeInclusive<usize>) -> impl Strategy<Value = String> {
    num_bytes_range.prop_flat_map(|num_bytes| {
        proptest::string::string_regex(&format!("0x[0-9a-fA-F]{{{}}}", 2 * num_bytes)).unwrap()
    })
}

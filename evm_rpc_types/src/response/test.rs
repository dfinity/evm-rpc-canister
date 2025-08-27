use crate::{Block, Hex, Hex20, Hex256, Hex32, LogEntry, Nat256};
use num_bigint::BigUint;
use proptest::{
    arbitrary::any, collection::vec, option, prelude::Strategy, prop_assert_eq, prop_compose,
    proptest,
};
use serde_json::Value;
use std::{ops::RangeInclusive, str::FromStr};

// To check conversion from `evm_rpc_types` to `alloy_rpc_types`, these tests generate an arbitrary
// (valid) type from the `evm_rpc_types` crate, convert it to the corresponding `alloy_rpc_types`
// type, and compare both serialized values.
// This is done so that we can check conversion for randomly generated values and not just a few
// hardcoded instances.
#[cfg(feature = "alloy")]
mod alloy_conversion_tests {
    use super::*;

    proptest! {
        #[test]
        fn should_convert_log_to_alloy(entry in arb_log_entry()) {
            let serialized = serde_json::to_value(&entry).unwrap();

            let mut alloy_serialized = serde_json::to_value(&alloy_rpc_types::Log::try_from(entry.clone()).unwrap()).unwrap();
            hex_to_u32_digits(&mut alloy_serialized, "transactionIndex");
            hex_to_u32_digits(&mut alloy_serialized, "logIndex");
            hex_to_u32_digits(&mut alloy_serialized, "blockNumber");

            prop_assert_eq!(serialized, alloy_serialized);
        }

        #[test]
        fn should_convert_block_to_alloy(block in arb_block()) {
            let serialized = serde_json::to_value(&block).unwrap();

            let mut alloy_serialized = serde_json::to_value(&alloy_rpc_types::Block::try_from(block.clone()).unwrap()).unwrap();
            hex_to_u32_digits(&mut alloy_serialized, "baseFeePerGas");
            hex_to_u32_digits(&mut alloy_serialized, "number");
            hex_to_u32_digits(&mut alloy_serialized, "difficulty");
            hex_to_u32_digits(&mut alloy_serialized, "gasLimit");
            hex_to_u32_digits(&mut alloy_serialized, "gasUsed");
            hex_to_u32_digits(&mut alloy_serialized, "nonce");
            hex_to_u32_digits(&mut alloy_serialized, "size");
            hex_to_u32_digits(&mut alloy_serialized, "timestamp");
            hex_to_u32_digits(&mut alloy_serialized, "totalDifficulty");
            add_null_if_absent(&mut alloy_serialized, "baseFeePerGas");
            add_null_if_absent(&mut alloy_serialized, "totalDifficulty");

            prop_assert_eq!(serialized, alloy_serialized);
        }
    }

    prop_compose! {
        fn arb_block()
        (
            base_fee_per_gas in option::of(arb_u64()),
            number in arb_u64(),
            difficulty in arb_nat256(),
            extra_data in arb_hex(),
            gas_limit in arb_u64(),
            gas_used in arb_u64(),
            hash in arb_hex32(),
            logs_bloom in arb_hex256(),
            miner in  arb_hex20(),
            mix_hash in arb_hex32(),
            nonce in arb_u64(),
            parent_hash in arb_hex32(),
            receipts_root in arb_hex32(),
            sha3_uncles in arb_hex32(),
            size in arb_u64(),
            state_root in arb_hex32(),
            timestamp in arb_u64(),
            total_difficulty in option::of(arb_nat256()),
            transactions in vec(arb_hex32(), 0..100),
            transactions_root in arb_hex32(),
            uncles in vec(arb_hex32(), 0..100),
        ) -> Block {
            Block {
                base_fee_per_gas,
                number,
                // alloy requires the `difficulty` field be present
                difficulty: Some(difficulty),
                extra_data,
                gas_limit,
                gas_used,
                hash,
                logs_bloom,
                miner,
                mix_hash,
                nonce,
                parent_hash,
                receipts_root,
                sha3_uncles,
                size,
                state_root,
                timestamp,
                total_difficulty,
                transactions,
                // alloy requires the `transactions_root` field be present
                transactions_root: Some(transactions_root),
                uncles,
            }
        }
    }

    prop_compose! {
        fn arb_log_entry()
        (
            address in arb_hex20(),
            topics in  vec(arb_hex32(), 0..=4),
            data in arb_hex(),
            block_number in option::of(arb_u64()),
            transaction_hash in option::of(arb_hex32()),
            transaction_index in option::of(arb_u64()),
            block_hash in option::of(arb_hex32()),
            log_index in option::of(arb_u64()),
            removed in any::<bool>(),
        ) -> LogEntry {
            LogEntry {
                address,
                topics,
                data,
                block_number,
                transaction_hash,
                transaction_index,
                block_hash,
                log_index,
                removed,
            }
        }
    }

    // `u64` wrapped in a `Nat256`
    fn arb_u64() -> impl Strategy<Value = Nat256> {
        any::<u64>().prop_map(Nat256::from)
    }

    fn arb_nat256() -> impl Strategy<Value = Nat256> {
        any::<[u8; 32]>().prop_map(Nat256::from_be_bytes)
    }

    fn arb_hex20() -> impl Strategy<Value = Hex20> {
        arb_var_len_hex_string(20..=20_usize).prop_map(|s| Hex20::from_str(s.as_str()).unwrap())
    }

    fn arb_hex32() -> impl Strategy<Value = Hex32> {
        arb_var_len_hex_string(32..=32_usize).prop_map(|s| Hex32::from_str(s.as_str()).unwrap())
    }

    fn arb_hex256() -> impl Strategy<Value = Hex256> {
        arb_var_len_hex_string(256..=256_usize).prop_map(|s| Hex256::from_str(s.as_str()).unwrap())
    }

    fn arb_hex() -> impl Strategy<Value = Hex> {
        arb_var_len_hex_string(0..=100_usize).prop_map(|s| Hex::from_str(s.as_str()).unwrap())
    }

    // This method checks if the given `serde_json::Value` contains the given field, and if so,
    // it parses its value as a hexadecimal string and converts it to an array of u32 digits.
    // This is needed to compare serialized values between `alloy_rpc_types` and `evm_rpc_types`
    // since the former serialized integers as hex strings, but the latter as arrays of u32 digits.
    fn hex_to_u32_digits(serialized: &mut Value, field: &str) {
        if let Some(Value::String(hex)) = serialized.get(field) {
            let hex = hex.strip_prefix("0x").unwrap_or(hex);
            let digits = BigUint::parse_bytes(hex.as_bytes(), 16)
                .unwrap()
                .to_u32_digits();
            serialized[field] = digits.into();
        }
    }

    // This method checks if the given `serde_json` contains the given field, and if not, sets its
    // value to `serde_json::Value::Null`.
    // This is needed to compare serialized values because some fields are skipped during
    // serialization in `alloy_rpc_types` but not `evm_rpc_types`
    fn add_null_if_absent(serialized: &mut Value, field: &str) {
        if serialized.get(field).is_none() {
            serialized[field] = Value::Null;
        }
    }
}

fn arb_var_len_hex_string(num_bytes_range: RangeInclusive<usize>) -> impl Strategy<Value = String> {
    num_bytes_range.prop_flat_map(|num_bytes| {
        proptest::string::string_regex(&format!("0x[0-9a-fA-F]{{{}}}", 2 * num_bytes)).unwrap()
    })
}

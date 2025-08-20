use crate::{Hex, Hex20, Hex32};

impl From<Hex20> for alloy_primitives::Address {
    fn from(value: Hex20) -> Self {
        Self::from(<[u8; 20]>::from(value))
    }
}

impl From<alloy_primitives::Address> for Hex20 {
    fn from(value: alloy_primitives::Address) -> Self {
        Self::from(value.into_array())
    }
}

impl From<Hex32> for alloy_primitives::B256 {
    fn from(value: Hex32) -> Self {
        Self::from(<[u8; 32]>::from(value))
    }
}

impl From<alloy_primitives::B256> for Hex32 {
    fn from(value: alloy_primitives::B256) -> Self {
        Self::from(value.0)
    }
}

impl From<Hex> for alloy_primitives::Bytes {
    fn from(value: Hex) -> Self {
        Self::from_iter(Vec::<u8>::from(value))
    }
}

impl From<alloy_primitives::Bytes> for Hex {
    fn from(value: alloy_primitives::Bytes) -> Self {
        Hex(value.to_vec())
    }
}

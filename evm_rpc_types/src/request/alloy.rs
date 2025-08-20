use crate::{BlockTag, GetLogsArgs, Hex20, RpcError};

impl From<alloy_rpc_types::BlockNumberOrTag> for BlockTag {
    fn from(tag: alloy_rpc_types::BlockNumberOrTag) -> Self {
        use alloy_rpc_types::BlockNumberOrTag;
        match tag {
            BlockNumberOrTag::Latest => Self::Latest,
            BlockNumberOrTag::Finalized => Self::Finalized,
            BlockNumberOrTag::Safe => Self::Safe,
            BlockNumberOrTag::Earliest => Self::Earliest,
            BlockNumberOrTag::Pending => Self::Pending,
            BlockNumberOrTag::Number(n) => Self::Number(n.into()),
        }
    }
}

impl TryFrom<BlockTag> for alloy_rpc_types::BlockNumberOrTag {
    type Error = RpcError;

    fn try_from(tag: BlockTag) -> Result<Self, Self::Error> {
        Ok(match tag {
            BlockTag::Latest => Self::Latest,
            BlockTag::Finalized => Self::Finalized,
            BlockTag::Safe => Self::Safe,
            BlockTag::Earliest => Self::Earliest,
            BlockTag::Pending => Self::Pending,
            BlockTag::Number(n) => Self::Number(u64::try_from(n)?),
        })
    }
}

impl<T: IntoIterator<Item = Hex20>> From<Vec<T>> for GetLogsArgs {
    fn from(addresses: Vec<T>) -> Self {
        Self {
            from_block: None,
            to_block: None,
            addresses: addresses.into_iter().map(Into::into).collect(),
            topics: None,
        }
    }
}

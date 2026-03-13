use crate::result::{BatchResult, ProviderError, RpcError, RpcResult};
use crate::{EthMainnetService, MultiRpcResult, RpcService, ValidationError};
use candid::{CandidType, Decode, Deserialize, Encode};

#[test]
fn test_multi_rpc_result_map() {
    let err = RpcError::ProviderError(ProviderError::ProviderNotFound);
    assert_eq!(
        MultiRpcResult::Consistent(Ok(5)).map(|n| n + 1),
        MultiRpcResult::Consistent(Ok(6))
    );
    assert_eq!(
        MultiRpcResult::Consistent(Err(err.clone())).map(|()| unreachable!()),
        MultiRpcResult::Consistent(Err(err.clone()))
    );
    assert_eq!(
        MultiRpcResult::Inconsistent(vec![
            (RpcService::EthMainnet(EthMainnetService::Ankr), Ok(5)),
            (RpcService::EthMainnet(EthMainnetService::Ankr), Ok(6))
        ])
        .map(|n| n + 1),
        MultiRpcResult::Inconsistent(vec![
            (RpcService::EthMainnet(EthMainnetService::Ankr), Ok(6)),
            (RpcService::EthMainnet(EthMainnetService::Ankr), Ok(7))
        ])
    );
    assert_eq!(
        MultiRpcResult::Inconsistent(vec![
            (RpcService::EthMainnet(EthMainnetService::Ankr), Ok(5)),
            (
                RpcService::EthMainnet(EthMainnetService::Cloudflare),
                Ok(10)
            )
        ])
        .map(|n| n + 1),
        MultiRpcResult::Inconsistent(vec![
            (RpcService::EthMainnet(EthMainnetService::Ankr), Ok(6)),
            (
                RpcService::EthMainnet(EthMainnetService::Cloudflare),
                Ok(11)
            )
        ])
    );
    assert_eq!(
        MultiRpcResult::Inconsistent(vec![
            (RpcService::EthMainnet(EthMainnetService::Ankr), Ok(5)),
            (
                RpcService::EthMainnet(EthMainnetService::PublicNode),
                Err(err.clone())
            )
        ])
        .map(|n| n + 1),
        MultiRpcResult::Inconsistent(vec![
            (RpcService::EthMainnet(EthMainnetService::Ankr), Ok(6)),
            (
                RpcService::EthMainnet(EthMainnetService::PublicNode),
                Err(err)
            )
        ])
    );
    assert_eq!(
        MultiRpcResult::Inconsistent(vec![(
            RpcService::EthMainnet(EthMainnetService::Ankr),
            Ok(2)
        )])
        .map(|n| n / 2),
        MultiRpcResult::Consistent(Ok(1))
    );
    assert_eq!(
        MultiRpcResult::Inconsistent(vec![
            (RpcService::EthMainnet(EthMainnetService::Ankr), Ok(2)),
            (RpcService::EthMainnet(EthMainnetService::Llama), Ok(3))
        ])
        .map(|n| n / 2),
        MultiRpcResult::Consistent(Ok(1))
    );
    assert_eq!(
        MultiRpcResult::Inconsistent(vec![
            (
                RpcService::EthMainnet(EthMainnetService::Ankr),
                Err(RpcError::ValidationError(ValidationError::Custom(
                    "error message".into()
                )))
            ),
            (
                RpcService::EthMainnet(EthMainnetService::Llama),
                Err(RpcError::ValidationError(ValidationError::Custom(
                    "error message".into()
                )))
            )
        ])
        .and_then(|()| unreachable!()),
        MultiRpcResult::Consistent::<()>(Err(RpcError::ValidationError(ValidationError::Custom(
            "error message".into()
        ))))
    );
}
#[test]
fn test_multi_rpc_result_and_then() {
    let err = RpcError::ProviderError(ProviderError::ProviderNotFound);
    assert_eq!(
        MultiRpcResult::Consistent(Ok(5)).and_then(|n| Ok(n + 1)),
        MultiRpcResult::Consistent(Ok(6))
    );
    assert_eq!(
        MultiRpcResult::Consistent(Err(err.clone())).and_then(|()| unreachable!()),
        MultiRpcResult::Consistent::<()>(Err(err.clone()))
    );
    assert_eq!(
        MultiRpcResult::Inconsistent(vec![
            (RpcService::EthMainnet(EthMainnetService::Ankr), Ok(5)),
            (RpcService::EthMainnet(EthMainnetService::Ankr), Ok(6))
        ])
        .and_then(|n| Ok(n + 1)),
        MultiRpcResult::Inconsistent(vec![
            (RpcService::EthMainnet(EthMainnetService::Ankr), Ok(6)),
            (RpcService::EthMainnet(EthMainnetService::Ankr), Ok(7))
        ])
    );
    assert_eq!(
        MultiRpcResult::Inconsistent(vec![
            (RpcService::EthMainnet(EthMainnetService::Ankr), Ok(5)),
            (
                RpcService::EthMainnet(EthMainnetService::Cloudflare),
                Ok(10)
            )
        ])
        .and_then(|n| Ok(n + 1)),
        MultiRpcResult::Inconsistent(vec![
            (RpcService::EthMainnet(EthMainnetService::Ankr), Ok(6)),
            (
                RpcService::EthMainnet(EthMainnetService::Cloudflare),
                Ok(11)
            )
        ])
    );
    assert_eq!(
        MultiRpcResult::Inconsistent(vec![
            (RpcService::EthMainnet(EthMainnetService::Ankr), Ok(5)),
            (
                RpcService::EthMainnet(EthMainnetService::PublicNode),
                Err(err.clone())
            )
        ])
        .and_then(|n| Ok(n + 1)),
        MultiRpcResult::Inconsistent(vec![
            (RpcService::EthMainnet(EthMainnetService::Ankr), Ok(6)),
            (
                RpcService::EthMainnet(EthMainnetService::PublicNode),
                Err(err.clone())
            )
        ])
    );
    assert_eq!(
        MultiRpcResult::Inconsistent(vec![
            (RpcService::EthMainnet(EthMainnetService::Ankr), Ok(1)),
            (RpcService::EthMainnet(EthMainnetService::Llama), Ok(2))
        ])
        .and_then(|n| if n % 2 == 0 { Ok(n) } else { Err(err.clone()) }),
        MultiRpcResult::Inconsistent(vec![
            (
                RpcService::EthMainnet(EthMainnetService::Ankr),
                Err(err.clone())
            ),
            (RpcService::EthMainnet(EthMainnetService::Llama), Ok(2)),
        ])
    );
    assert_eq!(
        MultiRpcResult::Inconsistent(vec![
            (RpcService::EthMainnet(EthMainnetService::Ankr), Ok(1)),
            (RpcService::EthMainnet(EthMainnetService::Llama), Ok(3))
        ])
        .and_then(|n| if n % 2 == 0 { Ok(n) } else { Err(err.clone()) }),
        MultiRpcResult::Consistent(Err(err.clone()))
    );
    assert_eq!(
        MultiRpcResult::Inconsistent(vec![
            (RpcService::EthMainnet(EthMainnetService::Ankr), Ok(2)),
            (RpcService::EthMainnet(EthMainnetService::Llama), Ok(3))
        ])
        .and_then(|n| Ok(n / 2)),
        MultiRpcResult::Consistent(Ok(1))
    );
    assert_eq!(
        MultiRpcResult::Inconsistent(vec![
            (
                RpcService::EthMainnet(EthMainnetService::Ankr),
                Err(RpcError::ValidationError(ValidationError::Custom(
                    "error message".into()
                )))
            ),
            (
                RpcService::EthMainnet(EthMainnetService::Llama),
                Err(RpcError::ValidationError(ValidationError::Custom(
                    "error message".into()
                )))
            )
        ])
        .and_then(|()| unreachable!()),
        MultiRpcResult::Consistent::<()>(Err(RpcError::ValidationError(ValidationError::Custom(
            "error message".into()
        ))))
    );
}

mod batch_result_backwards_compatibility {
    use super::*;
    use crate::{
        Block, FeeHistory, LogEntry, Nat256, SendRawTransactionStatus, TransactionReceipt,
    };

    /// Adding a new variant to `BatchResult` is NOT a breaking change in the Candid API
    /// because the new variant can only be returned if the corresponding new variant
    /// was requested in `BatchRequest`, which an old client would never do.
    ///
    /// This test verifies that an old client can still decode all pre-existing variants.
    #[test]
    fn old_client_can_decode_pre_existing_variants() {
        let err: RpcError = RpcError::ProviderError(ProviderError::ProviderNotFound);
        let pre_existing_variants: Vec<BatchResult> = vec![
            BatchResult::EthFeeHistory(Box::new(Err(err.clone()))),
            BatchResult::EthGetBlockByNumber(Box::new(Err(err.clone()))),
            BatchResult::EthGetLogs(Box::new(Err(err.clone()))),
            BatchResult::EthGetTransactionCount(Box::new(Err(err.clone()))),
            BatchResult::EthGetTransactionReceipt(Box::new(Err(err.clone()))),
            BatchResult::EthSendRawTransaction(Box::new(Err(err))),
        ];
        for variant in pre_existing_variants {
            let encoded = Encode!(&variant).unwrap();

            let decoded = Decode!(&encoded, OldBatchResult).unwrap();

            assert_eq!(decoded, OldBatchResult::try_from(variant).unwrap());
        }
    }

    /// `BatchResult` without the `EthCall` variant, simulating a client
    /// compiled against an older version of the Candid interface.
    #[derive(Clone, Debug, PartialEq, CandidType, Deserialize)]
    #[allow(clippy::large_enum_variant, clippy::enum_variant_names)] //test code
    enum OldBatchResult {
        EthFeeHistory(RpcResult<FeeHistory>),
        EthGetBlockByNumber(RpcResult<Block>),
        EthGetLogs(RpcResult<Vec<LogEntry>>),
        EthGetTransactionCount(RpcResult<Nat256>),
        EthGetTransactionReceipt(RpcResult<Option<TransactionReceipt>>),
        EthSendRawTransaction(RpcResult<SendRawTransactionStatus>),
    }

    impl TryFrom<BatchResult> for OldBatchResult {
        type Error = String;

        fn try_from(value: BatchResult) -> Result<Self, Self::Error> {
            // Exhaustive match ensures a compile-time error when a new variant is added,
            // reminding the developer to update this test.
            match value {
                BatchResult::EthFeeHistory(v) => Ok(Self::EthFeeHistory(*v)),
                BatchResult::EthGetBlockByNumber(v) => Ok(Self::EthGetBlockByNumber(*v)),
                BatchResult::EthGetLogs(v) => Ok(Self::EthGetLogs(*v)),
                BatchResult::EthGetTransactionCount(v) => Ok(Self::EthGetTransactionCount(*v)),
                BatchResult::EthGetTransactionReceipt(v) => Ok(Self::EthGetTransactionReceipt(*v)),
                BatchResult::EthSendRawTransaction(v) => Ok(Self::EthSendRawTransaction(*v)),
                // New variants not present in the old interface.
                BatchResult::EthCall(_) => Err("EthCall is not supported".to_string()),
            }
        }
    }
}

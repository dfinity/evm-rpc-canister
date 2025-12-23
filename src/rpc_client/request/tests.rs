use crate::rpc_client::request::process_result;
use crate::types::RpcMethod;
use canhttp::multi::MultiResults;
use evm_rpc_types::{EthMainnetService, MultiRpcResult, ProviderError, RpcError, RpcService};

#[test]
fn test_process_result_mapping() {
    type ReductionError = canhttp::multi::ReductionError<RpcService, u32, RpcError>;

    assert_eq!(
        process_result(RpcMethod::EthGetTransactionCount, Ok(5)),
        MultiRpcResult::Consistent(Ok(5))
    );
    assert_eq!(
        process_result(
            RpcMethod::EthGetTransactionCount,
            Err(ReductionError::ConsistentError(RpcError::ProviderError(
                ProviderError::MissingRequiredProvider
            )))
        ),
        MultiRpcResult::Consistent(Err(RpcError::ProviderError(
            ProviderError::MissingRequiredProvider
        )))
    );
    assert_eq!(
        process_result(
            RpcMethod::EthGetTransactionCount,
            Err(ReductionError::InconsistentResults(MultiResults::default()))
        ),
        MultiRpcResult::Inconsistent(vec![])
    );
    assert_eq!(
        process_result(
            RpcMethod::EthGetTransactionCount,
            Err(ReductionError::InconsistentResults(
                MultiResults::from_non_empty_iter(vec![(
                    RpcService::EthMainnet(EthMainnetService::Ankr),
                    Ok(5)
                )])
            ))
        ),
        MultiRpcResult::Inconsistent(vec![(
            RpcService::EthMainnet(EthMainnetService::Ankr),
            Ok(5)
        )])
    );
    assert_eq!(
        process_result(
            RpcMethod::EthGetTransactionCount,
            Err(ReductionError::InconsistentResults(
                MultiResults::from_non_empty_iter(vec![
                    (RpcService::EthMainnet(EthMainnetService::Ankr), Ok(5)),
                    (
                        RpcService::EthMainnet(EthMainnetService::Cloudflare),
                        Err(RpcError::ProviderError(ProviderError::NoPermission))
                    )
                ])
            ))
        ),
        MultiRpcResult::Inconsistent(vec![
            (RpcService::EthMainnet(EthMainnetService::Ankr), Ok(5)),
            (
                RpcService::EthMainnet(EthMainnetService::Cloudflare),
                Err(RpcError::ProviderError(ProviderError::NoPermission))
            )
        ])
    );
}

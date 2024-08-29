mod cketh_conversion;

use async_trait::async_trait;
use candid::Nat;
use cketh_common::{
    eth_rpc::{
        into_nat, Block, Hash, ProviderError, RpcError, SendRawTransactionResult, ValidationError,
    },
    eth_rpc_client::{
        providers::{RpcApi, RpcService},
        requests::GetTransactionCountParams,
        EthRpcClient as CkEthRpcClient, MultiCallError, RpcConfig, RpcTransport,
    },
    lifecycle::EthereumNetwork,
};
use ethers_core::{types::Transaction, utils::rlp};
use evm_rpc_types::Hex32;
use ic_cdk::api::management_canister::http_request::{CanisterHttpRequestArgument, HttpResponse};

use crate::{
    accounting::get_http_request_cost,
    add_metric_entry,
    constants::{
        DEFAULT_ETH_MAINNET_SERVICES, DEFAULT_ETH_SEPOLIA_SERVICES, DEFAULT_L2_MAINNET_SERVICES,
        ETH_GET_LOGS_MAX_BLOCKS,
    },
    http::http_request,
    providers::resolve_rpc_service,
    types::{
        candid_types::{self, SendRawTransactionStatus},
        MetricRpcHost, MetricRpcMethod, MultiRpcResult, ResolvedRpcService, RpcMethod, RpcResult,
        RpcServices,
    },
    util::hex_to_bytes,
};

#[derive(Clone, Debug, PartialEq, Eq)]
struct CanisterTransport;

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl RpcTransport for CanisterTransport {
    fn resolve_api(service: &RpcService) -> Result<RpcApi, ProviderError> {
        Ok(resolve_rpc_service(service.clone())?.api())
    }

    async fn http_request(
        service: &RpcService,
        method: &str,
        request: CanisterHttpRequestArgument,
        effective_response_size_estimate: u64,
    ) -> RpcResult<HttpResponse> {
        let service = resolve_rpc_service(service.clone())?;
        let cycles_cost = get_http_request_cost(
            request
                .body
                .as_ref()
                .map(|bytes| bytes.len() as u64)
                .unwrap_or_default(),
            effective_response_size_estimate,
        );
        let rpc_method = MetricRpcMethod(method.to_string());
        http_request(rpc_method, service, request, cycles_cost).await
    }
}

fn check_services<T>(services: Vec<T>) -> RpcResult<Vec<T>> {
    if services.is_empty() {
        Err(ProviderError::ProviderNotFound)?;
    }
    Ok(services)
}

fn get_rpc_client(
    source: RpcServices,
    config: RpcConfig,
) -> RpcResult<CkEthRpcClient<CanisterTransport>> {
    Ok(match source {
        RpcServices::Custom { chain_id, services } => CkEthRpcClient::new(
            EthereumNetwork(chain_id),
            Some(
                check_services(services)?
                    .into_iter()
                    .map(RpcService::Custom)
                    .collect(),
            ),
            config,
        ),
        RpcServices::EthMainnet(services) => CkEthRpcClient::new(
            EthereumNetwork::MAINNET,
            Some(
                check_services(services.unwrap_or_else(|| DEFAULT_ETH_MAINNET_SERVICES.to_vec()))?
                    .into_iter()
                    .map(RpcService::EthMainnet)
                    .collect(),
            ),
            config,
        ),
        RpcServices::EthSepolia(services) => CkEthRpcClient::new(
            EthereumNetwork::SEPOLIA,
            Some(
                check_services(services.unwrap_or_else(|| DEFAULT_ETH_SEPOLIA_SERVICES.to_vec()))?
                    .into_iter()
                    .map(RpcService::EthSepolia)
                    .collect(),
            ),
            config,
        ),
        RpcServices::ArbitrumOne(services) => CkEthRpcClient::new(
            EthereumNetwork::ARBITRUM,
            Some(
                check_services(services.unwrap_or_else(|| DEFAULT_L2_MAINNET_SERVICES.to_vec()))?
                    .into_iter()
                    .map(RpcService::ArbitrumOne)
                    .collect(),
            ),
            config,
        ),
        RpcServices::BaseMainnet(services) => CkEthRpcClient::new(
            EthereumNetwork::BASE,
            Some(
                check_services(services.unwrap_or_else(|| DEFAULT_L2_MAINNET_SERVICES.to_vec()))?
                    .into_iter()
                    .map(RpcService::BaseMainnet)
                    .collect(),
            ),
            config,
        ),
        RpcServices::OptimismMainnet(services) => CkEthRpcClient::new(
            EthereumNetwork::OPTIMISM,
            Some(
                check_services(services.unwrap_or_else(|| DEFAULT_L2_MAINNET_SERVICES.to_vec()))?
                    .into_iter()
                    .map(RpcService::OptimismMainnet)
                    .collect(),
            ),
            config,
        ),
    })
}

fn process_result<T>(method: RpcMethod, result: Result<T, MultiCallError<T>>) -> MultiRpcResult<T> {
    match result {
        Ok(value) => MultiRpcResult::Consistent(Ok(value)),
        Err(err) => match err {
            MultiCallError::ConsistentError(err) => MultiRpcResult::Consistent(Err(err)),
            MultiCallError::InconsistentResults(multi_call_results) => {
                multi_call_results.results.iter().for_each(|(service, _)| {
                    if let Ok(ResolvedRpcService::Provider(provider)) =
                        resolve_rpc_service(service.clone())
                    {
                        add_metric_entry!(
                            inconsistent_responses,
                            (
                                method.into(),
                                MetricRpcHost(
                                    provider
                                        .hostname()
                                        .unwrap_or_else(|| "(unknown)".to_string())
                                )
                            ),
                            1
                        )
                    }
                });
                MultiRpcResult::Inconsistent(multi_call_results.results.into_iter().collect())
            }
        },
    }
}

pub struct CandidRpcClient {
    client: CkEthRpcClient<CanisterTransport>,
}

impl CandidRpcClient {
    pub fn new(source: RpcServices, config: Option<RpcConfig>) -> RpcResult<Self> {
        Ok(Self {
            client: get_rpc_client(source, config.unwrap_or_default())?,
        })
    }

    pub async fn eth_get_logs(
        &self,
        args: evm_rpc_types::GetLogsArgs,
    ) -> MultiRpcResult<Vec<evm_rpc_types::LogEntry>> {
        use crate::candid_rpc::cketh_conversion::{from_log_entries, into_get_logs_param};

        if let (
            Some(evm_rpc_types::BlockTag::Number(from)),
            Some(evm_rpc_types::BlockTag::Number(to)),
        ) = (&args.from_block, &args.to_block)
        {
            let from = Nat::from(from.clone());
            let to = Nat::from(to.clone());
            let block_count = if to > from { to - from } else { from - to };
            if block_count > ETH_GET_LOGS_MAX_BLOCKS {
                return MultiRpcResult::Consistent(Err(ValidationError::Custom(format!(
                    "Requested {} blocks; limited to {} when specifying a start and end block",
                    block_count, ETH_GET_LOGS_MAX_BLOCKS
                ))
                .into()));
            }
        }
        process_result(
            RpcMethod::EthGetLogs,
            self.client.eth_get_logs(into_get_logs_param(args)).await,
        )
        .map(from_log_entries)
    }

    pub async fn eth_get_block_by_number(
        &self,
        block: candid_types::BlockTag,
    ) -> MultiRpcResult<Block> {
        process_result(
            RpcMethod::EthGetBlockByNumber,
            self.client.eth_get_block_by_number(block.into()).await,
        )
    }

    pub async fn eth_get_transaction_receipt(
        &self,
        hash: Hex32,
    ) -> MultiRpcResult<Option<evm_rpc_types::TransactionReceipt>> {
        use crate::candid_rpc::cketh_conversion::{from_transaction_receipt, into_hash};
        process_result(
            RpcMethod::EthGetTransactionReceipt,
            self.client
                .eth_get_transaction_receipt(into_hash(hash))
                .await,
        )
        .map(|option| option.map(from_transaction_receipt))
    }

    pub async fn eth_get_transaction_count(
        &self,
        args: candid_types::GetTransactionCountArgs,
    ) -> MultiRpcResult<candid::Nat> {
        let args: GetTransactionCountParams = match args.try_into() {
            Ok(args) => args,
            Err(err) => return MultiRpcResult::Consistent(Err(RpcError::from(err))),
        };
        process_result(
            RpcMethod::EthGetTransactionCount,
            self.client
                .eth_get_transaction_count(args)
                .await
                .reduce_with_equality(),
        )
        .map(|count| into_nat(count.into_inner()))
    }

    pub async fn eth_fee_history(
        &self,
        args: evm_rpc_types::FeeHistoryArgs,
    ) -> MultiRpcResult<evm_rpc_types::FeeHistory> {
        use crate::candid_rpc::cketh_conversion::{from_fee_history, into_fee_history_params};
        process_result(
            RpcMethod::EthFeeHistory,
            self.client
                .eth_fee_history(into_fee_history_params(args))
                .await,
        )
        .map(from_fee_history)
    }

    pub async fn eth_send_raw_transaction(
        &self,
        raw_signed_transaction_hex: String,
    ) -> MultiRpcResult<candid_types::SendRawTransactionStatus> {
        let transaction_hash = get_transaction_hash(&raw_signed_transaction_hex);
        process_result(
            RpcMethod::EthSendRawTransaction,
            self.client
                .multi_eth_send_raw_transaction(raw_signed_transaction_hex)
                .await,
        )
        .map(|result| match result {
            SendRawTransactionResult::Ok => SendRawTransactionStatus::Ok(transaction_hash),
            SendRawTransactionResult::InsufficientFunds => {
                SendRawTransactionStatus::InsufficientFunds
            }
            SendRawTransactionResult::NonceTooLow => SendRawTransactionStatus::NonceTooLow,
            SendRawTransactionResult::NonceTooHigh => SendRawTransactionStatus::NonceTooHigh,
        })
    }
}

fn get_transaction_hash(raw_signed_transaction_hex: &str) -> Option<Hash> {
    let bytes = hex_to_bytes(raw_signed_transaction_hex)?;
    let transaction: Transaction = rlp::decode(&bytes).ok()?;
    Some(Hash(transaction.hash.0))
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_process_result_mapping() {
        use cketh_common::eth_rpc_client::{providers::EthMainnetService, MultiCallResults};

        let method = RpcMethod::EthGetTransactionCount;

        assert_eq!(
            process_result(method, Ok(5)),
            MultiRpcResult::Consistent(Ok(5))
        );
        assert_eq!(
            process_result(
                method,
                Err(MultiCallError::<()>::ConsistentError(
                    RpcError::ProviderError(ProviderError::MissingRequiredProvider)
                ))
            ),
            MultiRpcResult::Consistent(Err(RpcError::ProviderError(
                ProviderError::MissingRequiredProvider
            )))
        );
        assert_eq!(
            process_result(
                method,
                Err(MultiCallError::<()>::InconsistentResults(
                    MultiCallResults {
                        results: Default::default()
                    }
                ))
            ),
            MultiRpcResult::Inconsistent(vec![])
        );
        assert_eq!(
            process_result(
                method,
                Err(MultiCallError::InconsistentResults(MultiCallResults {
                    results: vec![(RpcService::EthMainnet(EthMainnetService::Ankr), Ok(5))]
                        .into_iter()
                        .collect(),
                }))
            ),
            MultiRpcResult::Inconsistent(vec![(
                RpcService::EthMainnet(EthMainnetService::Ankr),
                Ok(5)
            )])
        );
        assert_eq!(
            process_result(
                method,
                Err(MultiCallError::InconsistentResults(MultiCallResults {
                    results: vec![
                        (RpcService::EthMainnet(EthMainnetService::Ankr), Ok(5)),
                        (
                            RpcService::EthMainnet(EthMainnetService::Cloudflare),
                            Err(RpcError::ProviderError(ProviderError::NoPermission))
                        )
                    ]
                    .into_iter()
                    .collect(),
                }))
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
}

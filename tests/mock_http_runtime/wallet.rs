//! Module to interact with a [cycles wallet](https://github.com/dfinity/cycles-wallet) canister.

use crate::mock_http_runtime::{decode_call_response, encode_args, MockHttpRuntime};
use async_trait::async_trait;
use candid::{utils::ArgumentEncoder, CandidType, Principal};
use evm_rpc_client::{IcError, Runtime};
use ic_error_types::RejectCode;
use ic_management_canister_types::CanisterId;
use regex::Regex;
use serde::{de::DeserializeOwned, Deserialize};

pub struct MockHttpRuntimeWithWallet {
    pub mock_http_runtime: MockHttpRuntime,
    pub wallet_canister_id: CanisterId,
}

#[async_trait]
impl Runtime for MockHttpRuntimeWithWallet {
    async fn update_call<In, Out>(
        &self,
        id: Principal,
        method: &str,
        args: In,
        cycles: u128,
    ) -> Result<Out, IcError>
    where
        In: ArgumentEncoder + Send,
        Out: CandidType + DeserializeOwned,
    {
        self.mock_http_runtime
            .update_call::<(WalletCall128Args,), Result<WalletCall128Result, String>>(
                self.wallet_canister_id,
                "wallet_call128",
                (WalletCall128Args::new(id, method, args, cycles),),
                0,
            )
            .await
            .and_then(decode_cycles_wallet_response)
    }

    async fn query_call<In, Out>(
        &self,
        id: Principal,
        method: &str,
        args: In,
    ) -> Result<Out, IcError>
    where
        In: ArgumentEncoder + Send,
        Out: CandidType + DeserializeOwned,
    {
        self.mock_http_runtime.query_call(id, method, args).await
    }
}

/// Argument to the cycles wallet canister `wallet_call128` method.
#[derive(CandidType, Deserialize)]
pub struct WalletCall128Args {
    canister: Principal,
    method_name: String,
    #[serde(with = "serde_bytes")]
    args: Vec<u8>,
    cycles: u128,
}

impl WalletCall128Args {
    pub fn new<In: ArgumentEncoder>(
        canister_id: CanisterId,
        method: impl ToString,
        args: In,
        cycles: u128,
    ) -> Self {
        Self {
            canister: canister_id,
            method_name: method.to_string(),
            args: encode_args(args),
            cycles,
        }
    }
}

/// Return type of the cycles wallet canister `wallet_call128` method.
#[derive(CandidType, Deserialize)]
pub struct WalletCall128Result {
    #[serde(with = "serde_bytes", rename = "return")]
    pub bytes: Vec<u8>,
}

/// The cycles wallet canister formats the rejection code and error message from the target
/// canister into a single string. Extract them back from the formatted string.
pub fn decode_cycles_wallet_response<Out>(
    result: Result<WalletCall128Result, String>,
) -> Result<Out, IcError>
where
    Out: CandidType + DeserializeOwned,
{
    match result {
        Ok(WalletCall128Result { bytes }) => decode_call_response(bytes),
        Err(message) => {
            match Regex::new(r"^An error happened during the call: (\d+): (.*)$")
                .unwrap()
                .captures(&message)
            {
                Some(captures) => {
                    let (_, [code, message]) = captures.extract();
                    Err(IcError::CallRejected {
                        code: code.parse::<u64>().unwrap().try_into().unwrap(),
                        message: message.to_string(),
                    })
                }
                None => Err(IcError::CallRejected {
                    code: RejectCode::SysFatal,
                    message: message.to_string(),
                }),
            }
        }
    }
}

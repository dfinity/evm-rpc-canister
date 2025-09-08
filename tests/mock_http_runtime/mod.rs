pub mod mock;

use crate::MAX_TICKS;
use async_trait::async_trait;
use candid::{decode_args, utils::ArgumentEncoder, CandidType, Principal};
use evm_rpc::constants::DEFAULT_MAX_RESPONSE_BYTES;
use evm_rpc_client::Runtime;
use ic_error_types::RejectCode;
use mock::MockHttpOutcalls;
use pocket_ic::{
    common::rest::{
        CanisterHttpReject, CanisterHttpRequest, CanisterHttpResponse, MockCanisterHttpResponse,
    },
    nonblocking::PocketIc,
    RejectResponse,
};
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

pub struct MockHttpRuntime {
    pub env: Arc<PocketIc>,
    pub caller: Principal,
    pub mocks: Mutex<MockHttpOutcalls>,
}

#[async_trait]
impl Runtime for MockHttpRuntime {
    async fn update_call<In, Out>(
        &self,
        id: Principal,
        method: &str,
        args: In,
        _cycles: u128,
    ) -> Result<Out, (RejectCode, String)>
    where
        In: ArgumentEncoder + Send,
        Out: CandidType + DeserializeOwned,
    {
        // Forward the call through the wallet canister to attach cycles
        let message_id = self
            .env
            .submit_call(id, self.caller, method, encode_args(args))
            .await
            .unwrap();
        self.execute_mocks().await;
        self.env
            .await_call(message_id)
            .await
            .map(decode_call_response)
            .map_err(parse_reject_response)?
    }

    async fn query_call<In, Out>(
        &self,
        id: Principal,
        method: &str,
        args: In,
    ) -> Result<Out, (RejectCode, String)>
    where
        In: ArgumentEncoder + Send,
        Out: CandidType + DeserializeOwned,
    {
        self.env
            .query_call(id, self.caller, method, encode_args(args))
            .await
            .map(decode_call_response)
            .map_err(parse_reject_response)?
    }
}

impl MockHttpRuntime {
    async fn execute_mocks(&self) {
        loop {
            let pending_requests = tick_until_http_requests(self.env.as_ref()).await;
            if let Some(request) = pending_requests.first() {
                let maybe_mock = {
                    let mut mocks = self.mocks.lock().unwrap();
                    mocks.pop_matching(request)
                };
                match maybe_mock {
                    Some(mock) => {
                        let mock_response = MockCanisterHttpResponse {
                            subnet_id: request.subnet_id,
                            request_id: request.request_id,
                            response: check_response_size(request, mock.response),
                            additional_responses: vec![],
                        };
                        self.env.mock_canister_http_response(mock_response).await;
                    }
                    None => {
                        let CanisterHttpRequest { ref body, .. } = request;
                        if let Ok(body) = serde_json::from_slice::<Value>(body) {
                            panic!("No mocks matching the request: {request:?} with body {body:?}");
                        }
                        panic!("No mocks matching the request: {request:?}");
                    }
                }
            } else {
                return;
            }
        }
    }
}

fn check_response_size(
    request: &CanisterHttpRequest,
    response: CanisterHttpResponse,
) -> CanisterHttpResponse {
    if let CanisterHttpResponse::CanisterHttpReply(reply) = &response {
        let max_response_bytes = request
            .max_response_bytes
            .unwrap_or(DEFAULT_MAX_RESPONSE_BYTES);
        if reply.body.len() as u64 > max_response_bytes {
            // Approximate replica behavior since headers are not accounted for.
            return CanisterHttpResponse::CanisterHttpReject(CanisterHttpReject {
                reject_code: RejectCode::SysFatal as u64,
                message: format!("Http body exceeds size limit of {max_response_bytes} bytes.",),
            });
        }
    }
    response
}

fn parse_reject_response(response: RejectResponse) -> (RejectCode, String) {
    use pocket_ic::RejectCode as PocketIcRejectCode;
    let rejection_code = match response.reject_code {
        PocketIcRejectCode::SysFatal => RejectCode::SysFatal,
        PocketIcRejectCode::SysTransient => RejectCode::SysTransient,
        PocketIcRejectCode::DestinationInvalid => RejectCode::DestinationInvalid,
        PocketIcRejectCode::CanisterReject => RejectCode::CanisterReject,
        PocketIcRejectCode::CanisterError => RejectCode::CanisterError,
        PocketIcRejectCode::SysUnknown => RejectCode::SysUnknown,
    };
    (rejection_code, response.reject_message)
}

pub fn encode_args<In: ArgumentEncoder>(args: In) -> Vec<u8> {
    candid::encode_args(args).expect("Failed to encode arguments.")
}

pub fn decode_call_response<Out>(bytes: Vec<u8>) -> Result<Out, (RejectCode, String)>
where
    Out: CandidType + DeserializeOwned,
{
    decode_args(&bytes).map(|(res,)| res).map_err(|e| {
        (
            RejectCode::CanisterError,
            format!(
                "failed to decode canister response as {}: {}",
                std::any::type_name::<Out>(),
                e
            ),
        )
    })
}

async fn tick_until_http_requests(env: &PocketIc) -> Vec<CanisterHttpRequest> {
    let mut requests = Vec::new();
    for _ in 0..MAX_TICKS {
        requests = env.get_canister_http().await;
        if !requests.is_empty() {
            break;
        }
        env.tick().await;
        env.advance_time(Duration::from_nanos(1)).await;
    }
    requests
}

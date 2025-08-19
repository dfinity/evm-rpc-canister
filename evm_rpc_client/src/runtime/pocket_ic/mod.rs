mod mock;

use crate::{ClientBuilder, Runtime};
use async_trait::async_trait;
use candid::{decode_args, utils::ArgumentEncoder, CandidType, Principal};
use ic_error_types::RejectCode;
pub use mock::{
    forever, once, MockOutcall, MockOutcallBody, MockOutcallBuilder, MockOutcallQueue,
    MockOutcallRepeat, RepeatExt,
};
use pocket_ic::common::rest::{
    CanisterHttpReject, CanisterHttpRequest, CanisterHttpResponse, MockCanisterHttpResponse,
};
use pocket_ic::nonblocking::PocketIc;
use pocket_ic::RejectResponse;
use serde::de::DeserializeOwned;
use std::iter;
use std::sync::Mutex;
use std::time::Duration;

const DEFAULT_MAX_RESPONSE_BYTES: u64 = 2_000_000;
const MAX_TICKS: usize = 10;

pub struct PocketIcRuntime<'a> {
    pub env: &'a PocketIc,
    pub caller: Principal,
    // This field is in a `Mutex` so we can use interior mutability to pop the next element from
    // the queue (i.e., perform a mutable operation) within the `Runtime::update_call` method which
    // takes an immutable reference to `self`.
    pub mocks: Mutex<MockOutcallQueue>,
    pub controller: Principal,
}

impl Clone for PocketIcRuntime<'_> {
    fn clone(&self) -> Self {
        Self {
            env: self.env,
            caller: self.caller,
            mocks: Mutex::new(self.mocks.lock().unwrap().clone()),
            controller: self.controller,
        }
    }
}

#[async_trait]
impl Runtime for PocketIcRuntime<'_> {
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
        self.execute_mock().await;
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

impl PocketIcRuntime<'_> {
    async fn execute_mock(&self) {
        if let Some(mock) = {
            let mut mocks = self.mocks.lock().unwrap();
            mocks.next()
        } {
            let mut responses = mock.responses.clone().into_iter();
            let requests = tick_until_http_request(self.env).await;

            for (request, response) in iter::zip(requests, responses.by_ref()) {
                mock.assert_matches(&request);
                let mock_response = MockCanisterHttpResponse {
                    subnet_id: request.subnet_id,
                    request_id: request.request_id,
                    response: check_response_size(&request, response),
                    additional_responses: vec![],
                };
                self.env.mock_canister_http_response(mock_response).await;
            }

            if responses.next().is_some() {
                panic!("no pending HTTP request")
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

async fn tick_until_http_request(env: &PocketIc) -> Vec<CanisterHttpRequest> {
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

impl ClientBuilder<PocketIcRuntime<'_>> {
    pub fn mock(self, outcall: impl Into<MockOutcall>, repeat: MockOutcallRepeat) -> Self {
        self.with_runtime(|r| {
            r.mocks.lock().unwrap().push(outcall, repeat);
            r
        })
    }
}

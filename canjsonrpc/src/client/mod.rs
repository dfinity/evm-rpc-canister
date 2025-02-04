use crate::cycles::{CyclesChargingStrategy, EstimateRequestCyclesCost};
use crate::http::{RequestBuilder, RequestError, ResponseError};
use crate::json::{JsonRpcRequest, JsonRpcResponse};
use ic_cdk::api::call::RejectionCode;
use ic_cdk::api::management_canister::http_request::{
    CanisterHttpRequestArgument, HttpMethod, HttpResponse,
};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::sync::Arc;
use url::Url;

#[derive(Clone)]
pub struct Client {
    config: Arc<ClientConfig>,
}

pub struct ClientConfig {
    request_cost: Arc<dyn EstimateRequestCyclesCost>,
    charging: CyclesChargingStrategy,
}

pub enum JsonRpcError {}

pub enum CallerError {
    InvalidUrl { reason: String },
}

pub enum HttpOutcallError {
    RequestError(RequestError),
    InsufficientCyclesError {
        expected: u128,
        received: u128,
    },
    IcError {
        code: RejectionCode,
        message: String,
    },
}

impl Client {
    pub async fn call<Params, Res>(
        &self,
        request: JsonRpcRequest<Params>,
        url: String,
    ) -> Result<JsonRpcResponse<Res>, JsonRpcError>
    where
        Params: Serialize,
        Res: DeserializeOwned,
    {
        // let request = CanisterHttpRequestArgument {
        //     url: url.clone(),
        //     max_response_bytes: Some(effective_size_estimate),
        //     method: HttpMethod::POST,
        //     headers: headers.clone(),
        //     body: Some(payload.as_bytes().to_vec()),
        //     transform: Some(TransformContext::from_name(
        //         "cleanup_response".to_owned(),
        //         transform_op,
        //     )),
        // };
        todo!()
    }

    pub fn post(&self, url: &str) -> RequestBuilder {
        RequestBuilder::new(self.clone(), HttpMethod::POST, url)
    }

    pub async fn execute_request(
        &self,
        request: CanisterHttpRequestArgument,
    ) -> Result<HttpResponse, HttpOutcallError> {
        let cycles_cost = self.config.request_cost.cycles_cost(&request);
        match self.config.charging {
            CyclesChargingStrategy::PaidByCaller => {
                let cycles_available = ic_cdk::api::call::msg_cycles_available128();
                if cycles_available < cycles_cost {
                    return Err(HttpOutcallError::InsufficientCyclesError {
                        expected: cycles_cost,
                        received: cycles_available,
                    }
                    .into());
                }
                assert_eq!(
                    ic_cdk::api::call::msg_cycles_accept128(cycles_cost),
                    cycles_cost
                );
            }
            CyclesChargingStrategy::PaidByCanister => {}
        };

        match ic_cdk::api::management_canister::http_request::http_request(request, cycles_cost)
            .await
        {
            Ok((response,)) => Ok(response),
            Err((code, message)) => Err(HttpOutcallError::IcError { code, message }.into()),
        }
    }
}

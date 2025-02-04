use crate::http::{RequestBuilder, RequestError, ResponseError};
use crate::json::{JsonRpcRequest, JsonRpcResponse};
use ic_cdk::api::management_canister::http_request::{CanisterHttpRequestArgument, HttpMethod, HttpResponse};
use serde::de::DeserializeOwned;
use serde::Serialize;
use url::Url;

#[derive(Clone)]
pub struct Client {}

pub enum JsonRpcError {}

pub enum CallerError {
    InvalidUrl { reason: String },
}

pub enum HttpOutcallError {
    RequestError(RequestError),
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
        todo!()
    }
}

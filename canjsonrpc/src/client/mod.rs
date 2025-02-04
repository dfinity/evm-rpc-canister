use crate::http::RequestBuilder;
use crate::json::{JsonRpcRequest, JsonRpcResponse};
use ic_cdk::api::management_canister::http_request::HttpMethod;
use serde::de::DeserializeOwned;
use serde::Serialize;
use url::Url;

pub struct JsonRpcClient {}

pub enum JsonRpcError {}

pub enum CallerError {
    InvalidUrl { reason: String },
}

impl JsonRpcClient {
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
        RequestBuilder::new(HttpMethod::POST, url)
    }
}

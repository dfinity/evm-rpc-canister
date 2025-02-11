use ic_cdk::api::management_canister::http_request::CanisterHttpRequestArgument as IcHttpRequest;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IcHttpRequestWithCycles {
    pub request: IcHttpRequest,
    pub cycles: u128,
}

pub trait MaxResponseBytesRequestExtension {
    fn set_max_response_bytes(&mut self, value: u64);
    fn get_max_response_bytes(&self) -> Option<u64>;
}

impl MaxResponseBytesRequestExtension for IcHttpRequest {
    fn set_max_response_bytes(&mut self, value: u64) {
        self.max_response_bytes = Some(value);
    }

    fn get_max_response_bytes(&self) -> Option<u64> {
        self.max_response_bytes
    }
}

use std::task::{Context, Poll};
use tower::{Layer, Service};

/// TODO
pub struct ObservabilityLayer<OnRequest, OnResponse, OnError> {
    on_request: OnRequest,
    on_response: OnResponse,
    on_error: OnError,
}

impl ObservabilityLayer<DefaultObserver, DefaultObserver, DefaultObserver> {
    /// TODO
    pub fn new() -> Self {
        Self {
            on_request: DefaultObserver,
            on_response: DefaultObserver,
            on_error: DefaultObserver,
        }
    }
}

impl<OnRequest, OnResponse, OnError> ObservabilityLayer<OnRequest, OnResponse, OnError> {
    /// TODO
    pub fn on_request<NewOnRequest>(
        self,
        new_on_request: NewOnRequest,
    ) -> ObservabilityLayer<NewOnRequest, OnResponse, OnError> {
        ObservabilityLayer {
            on_request: new_on_request,
            on_response: self.on_response,
            on_error: self.on_error,
        }
    }
}

impl<S, OnRequest, OnResponse, OnError> Layer<S>
    for ObservabilityLayer<OnRequest, OnResponse, OnError>
where
    OnRequest: Clone,
    OnResponse: Clone,
    OnError: Clone,
{
    type Service = Observability<S, OnRequest, OnResponse, OnError>;

    fn layer(&self, inner: S) -> Self::Service {
        Self::Service {
            inner,
            on_request: self.on_request.clone(),
            on_response: self.on_response.clone(),
            on_error: self.on_error.clone(),
        }
    }
}

/// TODO
pub struct Observability<S, OnRequest, OnResponse, OnError> {
    inner: S,
    on_request: OnRequest,
    on_response: OnResponse,
    on_error: OnError,
}

impl<S, Request, Response, OnRequest, OnResponse, OnError> Service<Request>
    for Observability<S, OnRequest, OnResponse, OnError>
where
    S: Service<Request, Response = Response>,
    OnRequest: Fn(&Request),
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request) -> Self::Future {
        (self.on_request)(&req);
        self.inner.call(req)
    }
}

pub trait Observer<T> {
    fn observe(&self, value: &T);
}

///TODO
#[derive(Clone, Debug)]
pub struct DefaultObserver;
impl<T> Observer<T> for DefaultObserver {
    fn observe(&self, _value: &T) {
        //NOP
    }
}

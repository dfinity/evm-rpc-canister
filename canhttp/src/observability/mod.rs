use pin_project::pin_project;
use std::future::Future;
use std::pin::Pin;
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

    /// TODO
    pub fn on_response<NewOnResponse>(
        self,
        new_on_response: NewOnResponse,
    ) -> ObservabilityLayer<OnRequest, NewOnResponse, OnError> {
        ObservabilityLayer {
            on_request: self.on_request,
            on_response: new_on_response,
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
    OnRequest: Observer<Request>,
    OnResponse: Observer<S::Response> + Clone,
    OnError: Observer<S::Error> + Clone,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = ResponseFuture<S::Future, OnResponse, OnError>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request) -> Self::Future {
        self.on_request.observe(&req);
        ResponseFuture {
            response_future: self.inner.call(req),
            on_response: self.on_response.clone(),
            on_error: self.on_error.clone(),
        }
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

impl<F, T> Observer<T> for F
where
    F: Fn(&T),
{
    fn observe(&self, value: &T) {
        self(value);
    }
}

#[pin_project]
pub struct ResponseFuture<F, OnResponse, OnError> {
    #[pin]
    response_future: F,
    #[pin]
    on_response: OnResponse,
    #[pin]
    on_error: OnError,
}

impl<F, OnResponse, OnError, Response, Error> Future for ResponseFuture<F, OnResponse, OnError>
where
    F: Future<Output = Result<Response, Error>>,
    OnResponse: Observer<Response>,
    OnError: Observer<Error>,
{
    type Output = Result<Response, Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let result_fut = this.response_future.poll(cx);
        match &result_fut {
            Poll::Ready(result) => match result {
                Ok(response) => {
                    this.on_response.observe(response);
                }
                Err(error) => {
                    this.on_error.observe(error);
                }
            },
            Poll::Pending => {}
        }
        result_fut
    }
}

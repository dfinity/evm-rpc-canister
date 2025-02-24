//! Middleware that adds high level observability (e.g., logging, metrics) to a [`Service`].
//!
//! # Comparison with the `Trace` service of the [`tower_http`] crate.
//! This middleware is strongly inspired by the functionality offered by `Trace`.
//! The reason for not using this middleware directly is it cannot be used inside a canister:
//! 1. It measures the latency of a call by calling
//!     [`Instant::now`](https://github.com/tower-rs/tower-http/blob/469bdac3193ed22da9ea524a454d8cda93ffa0d5/tower-http/src/trace/service.rs#L302),
//!     which will fail when run from a canister.
//! 2. It can deal with streaming responses, which is unnecessary for HTTPs outcalls,
//!     since the response is available to a canister at once. This flexibility brings some complexity
//!     (body can only be fetched asynchronously, end of stream errors, etc.) which is not useful in a canister environment.
//!
//! [`Service`]: tower::Service
//! [`tower_http`]: https://crates.io/crates/tower-http

use pin_project::pin_project;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use tower::{Layer, Service};

/// [`Layer`] that adds high level observability to a [`Service`].
///
/// See the [module docs](crate::observability) for more details.
///
/// [`Layer`]: tower::Layer
/// [`Service`]: tower::Service
pub struct ObservabilityLayer<OnRequest, OnResponse, OnError> {
    on_request: OnRequest,
    on_response: OnResponse,
    on_error: OnError,
}

impl ObservabilityLayer<(), (), ()> {
    /// Creates a new [`ObservabilityLayer`] that does nothing.
    pub fn new() -> Self {
        Self {
            on_request: (),
            on_response: (),
            on_error: (),
        }
    }
}

impl Default for ObservabilityLayer<(), (), ()> {
    fn default() -> Self {
        Self::new()
    }
}

impl<OnRequest, OnResponse, OnError> ObservabilityLayer<OnRequest, OnResponse, OnError> {
    /// Customize what to do when a request is received.
    ///
    /// `NewOnRequest` is expected to implement [`RequestObserver`].
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

    /// Customize what to do when a response has been produced.
    ///
    /// `NewOnResponse` is expected to implement [`ResponseObserver`].
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

    /// Customize what to do when an error has been produced.
    ///
    /// `NewOnError` is expected to implement [`ResponseObserver`].
    pub fn on_error<NewOnError>(
        self,
        new_on_error: NewOnError,
    ) -> ObservabilityLayer<OnRequest, OnResponse, NewOnError> {
        ObservabilityLayer {
            on_request: self.on_request,
            on_response: self.on_response,
            on_error: new_on_error,
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

/// Middleware that adds high level observability to a [`Service`].
///
/// See the [module docs](crate::observability) for an example.
///
/// [`Service`]: tower::Service
pub struct Observability<S, OnRequest, OnResponse, OnError> {
    inner: S,
    on_request: OnRequest,
    on_response: OnResponse,
    on_error: OnError,
}

impl<S, Request, Response, OnRequest, RequestData, OnResponse, OnError> Service<Request>
    for Observability<S, OnRequest, OnResponse, OnError>
where
    S: Service<Request, Response = Response>,
    OnRequest: RequestObserver<Request, ObservableRequestData = RequestData>,
    OnResponse: ResponseObserver<RequestData, S::Response> + Clone,
    OnError: ResponseObserver<RequestData, S::Error> + Clone,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = ResponseFuture<S::Future, RequestData, OnResponse, OnError>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let req_data = self.on_request.observe_request(&req);
        ResponseFuture {
            response_future: self.inner.call(req),
            request_data: Some(req_data),
            on_response: self.on_response.clone(),
            on_error: self.on_error.clone(),
        }
    }
}

/// Trait used to tell [`Observability`] what to do when a request is received.
pub trait RequestObserver<Request> {
    /// Type of data that can be observed from the request (e.g., URL, host, etc.)
    /// when the response will be processed.
    type ObservableRequestData;

    /// Observe the given request and produce observable data based on the request.
    /// This observable data will be passed on to the response observer.
    fn observe_request(&self, request: &Request) -> Self::ObservableRequestData;
}

impl<Request> RequestObserver<Request> for () {
    type ObservableRequestData = ();

    fn observe_request(&self, _request: &Request) -> Self::ObservableRequestData {
        //NOP
    }
}

impl<F, Request, RequestData> RequestObserver<Request> for F
where
    F: Fn(&Request) -> RequestData,
{
    type ObservableRequestData = RequestData;

    fn observe_request(&self, request: &Request) -> Self::ObservableRequestData {
        self(request)
    }
}

/// Trait used to tell [`Observability`] what to do when a response is received.
pub trait ResponseObserver<RequestData, Result> {
    /// Observe the response (typically an instance of [`std::result::Result`] and the request data produced by a [`RequestObserver`].
    fn observe(&self, request_data: RequestData, value: &Result);
}

impl<ReqData, Result> ResponseObserver<ReqData, Result> for () {
    fn observe(&self, _request_data: ReqData, _value: &Result) {
        //NOP
    }
}

impl<F, ReqData, T> ResponseObserver<ReqData, T> for F
where
    F: Fn(ReqData, &T),
{
    fn observe(&self, request_data: ReqData, value: &T) {
        self(request_data, value);
    }
}

/// Response future for [`Observability`].
#[pin_project]
pub struct ResponseFuture<F, RequestData, OnResponse, OnError> {
    #[pin]
    response_future: F,
    request_data: Option<RequestData>,
    on_response: OnResponse,
    on_error: OnError,
}

impl<F, RequestData, OnResponse, OnError, Response, Error> Future
    for ResponseFuture<F, RequestData, OnResponse, OnError>
where
    F: Future<Output = Result<Response, Error>>,
    OnResponse: ResponseObserver<RequestData, Response>,
    OnError: ResponseObserver<RequestData, Error>,
{
    type Output = Result<Response, Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let result_fut = this.response_future.poll(cx);
        match &result_fut {
            Poll::Ready(result) => {
                let request_data = this.request_data.take().unwrap();
                match result {
                    Ok(response) => {
                        this.on_response.observe(request_data, response);
                    }
                    Err(error) => {
                        this.on_error.observe(request_data, error);
                    }
                }
            }
            Poll::Pending => {}
        }
        result_fut
    }
}

use pin_project::pin_project;
use std::fmt::Debug;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use tower::{Service, ServiceBuilder};
use tower_layer::{Layer, Stack};

pub trait Convert<Input> {
    type Output;
    type Error;

    fn try_convert(&mut self, response: Input) -> Result<Self::Output, Self::Error>;
}

#[derive(Debug, Clone)]
pub struct ConvertResponseLayer<F> {
    filter: F,
}

#[derive(Debug, Clone)]
pub struct ConvertResponse<S, F> {
    inner: S,
    filter: F,
}

impl<S, F: Clone> Layer<S> for ConvertResponseLayer<F> {
    type Service = ConvertResponse<S, F>;

    fn layer(&self, inner: S) -> Self::Service {
        Self::Service {
            inner,
            filter: self.filter.clone(),
        }
    }
}

impl<S, Request, Response, NewResponse, F> Service<Request> for ConvertResponse<S, F>
where
    S: Service<Request, Response = Response>,
    F: Convert<Response, Output = NewResponse> + Clone,
    F::Error: Into<S::Error>,
{
    type Response = NewResponse;
    type Error = S::Error;
    type Future = ResponseFuture<S::Future, F>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request) -> Self::Future {
        ResponseFuture {
            response_future: self.inner.call(req),
            filter: self.filter.clone(),
        }
    }
}

#[pin_project]
pub struct ResponseFuture<F, Filter> {
    #[pin]
    response_future: F,
    filter: Filter,
}

impl<F, Filter, Response, NewResponse, Error> Future for ResponseFuture<F, Filter>
where
    F: Future<Output = Result<Response, Error>>,
    Filter: Convert<Response, Output = NewResponse>,
    Filter::Error: Into<Error>,
{
    type Output = Result<NewResponse, Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let result_fut = this.response_future.poll(cx);
        match result_fut {
            Poll::Ready(result) => match result {
                Ok(response) => Poll::Ready(this.filter.try_convert(response).map_err(Into::into)),
                Err(err) => Poll::Ready(Err(err)),
            },
            Poll::Pending => Poll::Pending,
        }
    }
}

pub trait ConvertResponseServiceBuilder<L> {
    fn convert_response<F>(self, f: F) -> ServiceBuilder<Stack<ConvertResponseLayer<F>, L>>;
}

impl<L> ConvertResponseServiceBuilder<L> for ServiceBuilder<L> {
    fn convert_response<F>(self, f: F) -> ServiceBuilder<Stack<ConvertResponseLayer<F>, L>> {
        self.layer(ConvertResponseLayer { filter: f })
    }
}

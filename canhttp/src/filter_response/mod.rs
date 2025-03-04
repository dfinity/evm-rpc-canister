use pin_project::pin_project;
use std::fmt::Debug;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use tower::{Service, ServiceBuilder};
use tower_layer::{Layer, Stack};

pub trait FilterResponse<Response> {
    type Response;
    type Error;

    fn filter(&mut self, response: Response) -> Result<Self::Response, Self::Error>;
}

#[derive(Debug, Clone)]
pub struct FilterResponseLayer<F> {
    filter: F,
}

#[derive(Debug, Clone)]
pub struct FilterResponseService<S, F> {
    inner: S,
    filter: F,
}

impl<S, F: Clone> Layer<S> for FilterResponseLayer<F> {
    type Service = FilterResponseService<S, F>;

    fn layer(&self, inner: S) -> Self::Service {
        Self::Service {
            inner,
            filter: self.filter.clone(),
        }
    }
}

impl<S, Request, Response, NewResponse, F> Service<Request> for FilterResponseService<S, F>
where
    S: Service<Request, Response = Response>,
    F: FilterResponse<Response, Response = NewResponse> + Clone,
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
    Filter: FilterResponse<Response, Response = NewResponse>,
    Filter::Error: Into<Error>,
{
    type Output = Result<NewResponse, Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let result_fut = this.response_future.poll(cx);
        match result_fut {
            Poll::Ready(result) => match result {
                Ok(response) => Poll::Ready(this.filter.filter(response).map_err(Into::into)),
                Err(err) => Poll::Ready(Err(err)),
            },
            Poll::Pending => Poll::Pending,
        }
    }
}

pub trait FilterResponseServiceBuilder<L> {
    fn filter_response<F>(self, f: F) -> ServiceBuilder<Stack<FilterResponseLayer<F>, L>>;
}

impl<L> FilterResponseServiceBuilder<L> for ServiceBuilder<L> {
    fn filter_response<F>(self, f: F) -> ServiceBuilder<Stack<FilterResponseLayer<F>, L>> {
        self.layer(FilterResponseLayer { filter: f })
    }
}

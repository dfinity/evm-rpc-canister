use futures_util::future;
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
pub struct ConvertResponseLayer<C> {
    converter: C,
}

impl<C> ConvertResponseLayer<C> {
    pub fn new(converter: C) -> Self {
        Self { converter }
    }
}

#[derive(Debug, Clone)]
pub struct ConvertResponse<S, C> {
    inner: S,
    converter: C,
}

impl<S, C: Clone> Layer<S> for ConvertResponseLayer<C> {
    type Service = ConvertResponse<S, C>;

    fn layer(&self, inner: S) -> Self::Service {
        Self::Service {
            inner,
            converter: self.converter.clone(),
        }
    }
}

impl<S, Request, Response, NewResponse, Converter> Service<Request>
    for ConvertResponse<S, Converter>
where
    S: Service<Request, Response = Response>,
    Converter: Convert<Response, Output = NewResponse> + Clone,
    Converter::Error: Into<S::Error>,
{
    type Response = NewResponse;
    type Error = S::Error;
    type Future = ResponseFuture<S::Future, Converter>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request) -> Self::Future {
        ResponseFuture {
            response_future: self.inner.call(req),
            converter: self.converter.clone(),
        }
    }
}

#[pin_project]
pub struct ResponseFuture<F, Converter> {
    #[pin]
    response_future: F,
    converter: Converter,
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
                Ok(response) => {
                    Poll::Ready(this.converter.try_convert(response).map_err(Into::into))
                }
                Err(err) => Poll::Ready(Err(err)),
            },
            Poll::Pending => Poll::Pending,
        }
    }
}

pub trait ConvertServiceBuilder<L> {
    fn convert_request<C>(self, f: C) -> ServiceBuilder<Stack<ConvertRequestLayer<C>, L>>;
    fn convert_response<C>(self, f: C) -> ServiceBuilder<Stack<ConvertResponseLayer<C>, L>>;
}

impl<L> ConvertServiceBuilder<L> for ServiceBuilder<L> {
    fn convert_request<C>(self, converter: C) -> ServiceBuilder<Stack<ConvertRequestLayer<C>, L>> {
        self.layer(ConvertRequestLayer { converter })
    }

    fn convert_response<C>(
        self,
        converter: C,
    ) -> ServiceBuilder<Stack<ConvertResponseLayer<C>, L>> {
        self.layer(ConvertResponseLayer { converter })
    }
}

#[derive(Debug, Clone)]
pub struct ConvertRequestLayer<C> {
    converter: C,
}

impl<C> ConvertRequestLayer<C> {
    pub fn new(converter: C) -> Self {
        Self { converter }
    }
}

#[derive(Debug, Clone)]
pub struct ConvertRequest<S, C> {
    inner: S,
    converter: C,
}

impl<S, C: Clone> Layer<S> for ConvertRequestLayer<C> {
    type Service = ConvertRequest<S, C>;

    fn layer(&self, inner: S) -> Self::Service {
        Self::Service {
            inner,
            converter: self.converter.clone(),
        }
    }
}

impl<S, Converter, Request, NewRequest, Error> Service<NewRequest> for ConvertRequest<S, Converter>
where
    Converter: Convert<NewRequest, Output = Request>,
    S: Service<Request, Error = Error>,
    Converter::Error: Into<Error>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = future::Either<S::Future, future::Ready<Result<S::Response, S::Error>>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, new_req: NewRequest) -> Self::Future {
        match self.converter.try_convert(new_req) {
            Ok(request) => future::Either::Left(self.inner.call(request)),
            Err(err) => future::Either::Right(future::ready(Err(err.into()))),
        }
    }
}

use futures_util::future;
use pin_project::pin_project;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use tower::Service;
use tower_layer::Layer;

pub trait Validator<Request, Response> {
    type AssociatedRequestData;
    type Error;

    fn validate_request(
        &self,
        request: &Request,
    ) -> Result<Self::AssociatedRequestData, Self::Error>;

    fn validate_response(
        &mut self,
        request_data: Self::AssociatedRequestData,
        response: &Response,
    ) -> Result<(), Self::Error>;
}

#[derive(Debug, Clone)]
pub struct ValidatorLayer<V> {
    validator: V,
}

impl<S, V: Clone> Layer<S> for ValidatorLayer<V> {
    type Service = ValidatorService<S, V>;

    fn layer(&self, inner: S) -> Self::Service {
        Self::Service {
            inner,
            validator: self.validator.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ValidatorService<S, V> {
    inner: S,
    validator: V,
}

impl<S, V, Request, Response, ValidatedRequestData> Service<Request> for ValidatorService<S, V>
where
    S: Service<Request, Response = Response>,
    V: Validator<Request, Response, AssociatedRequestData = ValidatedRequestData> + Clone,
    V::Error: Into<S::Error>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = future::Either<
        ResponseFuture<S::Future, ValidatedRequestData, V>,
        future::Ready<Result<S::Response, S::Error>>,
    >;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request) -> Self::Future {
        match self.validator.validate_request(&req) {
            Ok(request_data) => future::Either::Left(ResponseFuture {
                response_future: self.inner.call(req),
                request_data: Some(request_data),
                validator: self.validator.clone(),
            }),
            Err(error) => future::Either::Right(future::ready(Err(error.into()))),
        }
    }
}

#[pin_project]
pub struct ResponseFuture<F, RequestData, Validator> {
    #[pin]
    response_future: F,
    request_data: Option<RequestData>,
    validator: Validator,
}

impl<F, V, Request, RequestData, Response, Error> Future for ResponseFuture<F, RequestData, V>
where
    F: Future<Output = Result<Response, Error>>,
    V: Validator<Request, Response, AssociatedRequestData = RequestData>,
    V::Error: Into<Error>,
{
    type Output = Result<Response, Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let result_fut = this.response_future.poll(cx);
        match result_fut {
            Poll::Ready(result) => match result {
                Ok(response) => {
                    let request_data = this.request_data.take().unwrap();
                    let validation_result =
                        match this.validator.validate_response(request_data, &response) {
                            Ok(()) => Ok(response),
                            Err(e) => Err(e.into()),
                        };
                    Poll::Ready(validation_result)
                }
                Err(err) => Poll::Ready(Err(err)),
            },
            Poll::Pending => Poll::Pending,
        }
    }
}

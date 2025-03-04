pub use request::{ConvertRequest, ConvertRequestLayer};
pub use response::{ConvertResponse, ConvertResponseLayer};

mod request;
mod response;

use tower::ServiceBuilder;
use tower_layer::Stack;

pub trait Convert<Input> {
    type Output;
    type Error;

    fn try_convert(&mut self, response: Input) -> Result<Self::Output, Self::Error>;
}

pub trait ConvertServiceBuilder<L> {
    fn convert_request<C>(self, f: C) -> ServiceBuilder<Stack<ConvertRequestLayer<C>, L>>;
    fn convert_response<C>(self, f: C) -> ServiceBuilder<Stack<ConvertResponseLayer<C>, L>>;
}

impl<L> ConvertServiceBuilder<L> for ServiceBuilder<L> {
    fn convert_request<C>(self, converter: C) -> ServiceBuilder<Stack<ConvertRequestLayer<C>, L>> {
        self.layer(ConvertRequestLayer::new(converter))
    }

    fn convert_response<C>(
        self,
        converter: C,
    ) -> ServiceBuilder<Stack<ConvertResponseLayer<C>, L>> {
        self.layer(ConvertResponseLayer::new(converter))
    }
}

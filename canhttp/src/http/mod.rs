//! HTTP translation layer

use ic_cdk::api::management_canister::http_request::TransformContext;

pub type HttpRequest = http::Request<Vec<u8>>;

pub trait MaxResponseBytesRequestExtension {
    fn set_max_response_bytes(&mut self, value: u64);
    fn get_max_response_bytes(&self) -> Option<u64>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct MaxResponseBytesExtension(pub u64);

impl<T> MaxResponseBytesRequestExtension for http::Request<T> {
    fn set_max_response_bytes(&mut self, value: u64) {
        let extensions = self.extensions_mut();
        extensions.insert(MaxResponseBytesExtension(value));
    }

    fn get_max_response_bytes(&self) -> Option<u64> {
        self.extensions()
            .get::<MaxResponseBytesExtension>()
            .map(|e| e.0)
    }
}

impl MaxResponseBytesRequestExtension for http::request::Builder {
    fn set_max_response_bytes(&mut self, value: u64) {
        if let Some(extensions) = self.extensions_mut() {
            extensions.insert(MaxResponseBytesExtension(value));
        }
    }

    fn get_max_response_bytes(&self) -> Option<u64> {
        self.extensions_ref()
            .and_then(|extensions| extensions.get::<MaxResponseBytesExtension>().map(|e| e.0))
    }
}

/// Convenience trait to follow the builder pattern.
pub trait MaxResponseBytesRequestExtensionBuilder {
    /// See [`MaxResponseBytesRequestExtension::set_max_response_bytes`].
    fn max_response_bytes(self, value: u64) -> Self;
}

impl<T> MaxResponseBytesRequestExtensionBuilder for T
where
    T: MaxResponseBytesRequestExtension,
{
    fn max_response_bytes(mut self, value: u64) -> Self {
        self.set_max_response_bytes(value);
        self
    }
}

pub trait TransformContextRequestExtension {
    fn set_transform_context(&mut self, value: TransformContext);
    fn get_transform_context(&self) -> Option<&TransformContext>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TransformContextExtension(pub TransformContext);

impl<T> TransformContextRequestExtension for http::Request<T> {
    fn set_transform_context(&mut self, value: TransformContext) {
        let extensions = self.extensions_mut();
        extensions.insert(TransformContextExtension(value));
    }

    fn get_transform_context(&self) -> Option<&TransformContext> {
        self.extensions()
            .get::<TransformContextExtension>()
            .map(|e| &e.0)
    }
}

impl TransformContextRequestExtension for http::request::Builder {
    fn set_transform_context(&mut self, value: TransformContext) {
        if let Some(extensions) = self.extensions_mut() {
            extensions.insert(TransformContextExtension(value));
        }
    }

    fn get_transform_context(&self) -> Option<&TransformContext> {
        self.extensions_ref()
            .and_then(|extensions| extensions.get::<TransformContextExtension>().map(|e| &e.0))
    }
}

/// Convenience trait to follow the builder pattern.
pub trait TransformContextRequestExtensionBuilder {
    /// See [`TransformContextRequestExtension::set_transform_context`].
    fn transform_context(self, value: TransformContext) -> Self;
}

impl<T> TransformContextRequestExtensionBuilder for T
where
    T: TransformContextRequestExtension,
{
    fn transform_context(mut self, value: TransformContext) -> Self {
        self.set_transform_context(value);
        self
    }
}

#[cfg(feature = "bincode")]
pub mod bincode;
#[cfg(feature = "bitcode")]
pub mod bitcode;
pub mod client;
mod error;
mod msg;
#[cfg(feature = "prost")]
pub mod prost;
pub mod server;

pub use client::ZrpcClient;
pub use error::ZrpcError;
pub use error::ZrpcServerError;
pub use error::ZrpcSystemError;
pub use msg::MsgSerde;
pub use server::{ZrpcService, ZrpcServiceHander};

#[cfg(test)]
mod test {}

/// A trait that defines the configuration for ZRPC types. This trait ensures that all associated types
/// are thread-safe by requiring them to implement `Send` and `Sync`.
pub trait ZrpcTypeConfig: Send + Sync {
    /// The input type for the ZRPC configuration.
    type In: Send + Sync;

    /// The output type for the ZRPC configuration.
    type Out: Send + Sync;

    /// The error type for the ZRPC configuration.
    type Err: Send + Sync;

    /// The wrapper type for the ZRPC configuration.
    type Wrapper: Send + Sync;

    /// The serializer/deserializer for the input type.
    type InSerde: MsgSerde<Data = Self::In>;

    /// The serializer/deserializer for the output type.
    type OutSerde: MsgSerde<Data = Self::Out>;

    /// The serializer/deserializer for the error wrapper type.
    type ErrSerde: MsgSerde<Data = Self::Wrapper>;

    /// Wraps a `ZrpcServerError` into the configured wrapper type.
    ///
    /// # Parameters
    /// - `output`: The `ZrpcServerError` to be wrapped.
    ///
    /// # Returns
    /// A wrapped `ZrpcServerError` of the configured wrapper type.
    fn wrap(output: ZrpcServerError<Self::Err>) -> Self::Wrapper;

    /// Unwraps a configured wrapper type into a `ZrpcServerError`.
    ///
    /// # Parameters
    /// - `wrapper`: The wrapped `ZrpcServerError` to be unwrapped.
    ///
    /// # Returns
    /// A `ZrpcServerError` of the configured error type.
    fn unwrap(wrapper: Self::Wrapper) -> ZrpcServerError<Self::Err>;
}

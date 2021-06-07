//! Async functions that can be used to handle requests.

use crate::{
    body::{Body, BoxBody},
    extract::FromRequest,
    response::IntoResponse,
    routing::{BoxResponseBody, EmptyRouter, MethodFilter, RouteFuture},
    service::HandleError,
};
use async_trait::async_trait;
use bytes::Bytes;
use futures_util::future;
use http::{Request, Response};
use std::{
    convert::Infallible,
    fmt,
    future::Future,
    marker::PhantomData,
    task::{Context, Poll},
};
use tower::{BoxError, Layer, Service, ServiceExt};

/// Route requests to the given handler regardless of the HTTP method of the
/// request.
///
/// # Example
///
/// ```rust
/// use tower_web::prelude::*;
///
/// async fn handler(request: Request<Body>) {}
///
/// // All requests to `/` will go to `handler` regardless of the HTTP method.
/// let app = route("/", any(handler));
/// ```
pub fn any<H, T>(handler: H) -> OnMethod<IntoService<H, T>, EmptyRouter>
where
    H: Handler<T>,
{
    on(MethodFilter::Any, handler)
}

/// Route `CONNECT` requests to the given handler.
///
/// See [`get`] for an example.
pub fn connect<H, T>(handler: H) -> OnMethod<IntoService<H, T>, EmptyRouter>
where
    H: Handler<T>,
{
    on(MethodFilter::Connect, handler)
}

/// Route `DELETE` requests to the given handler.
///
/// See [`get`] for an example.
pub fn delete<H, T>(handler: H) -> OnMethod<IntoService<H, T>, EmptyRouter>
where
    H: Handler<T>,
{
    on(MethodFilter::Delete, handler)
}

/// Route `GET` requests to the given handler.
///
/// # Example
///
/// ```rust
/// use tower_web::prelude::*;
///
/// async fn handler(request: Request<Body>) {}
///
/// // Requests to `GET /` will go to `handler`.
/// let app = route("/", get(handler));
/// ```
pub fn get<H, T>(handler: H) -> OnMethod<IntoService<H, T>, EmptyRouter>
where
    H: Handler<T>,
{
    on(MethodFilter::Get, handler)
}

/// Route `HEAD` requests to the given handler.
///
/// See [`get`] for an example.
pub fn head<H, T>(handler: H) -> OnMethod<IntoService<H, T>, EmptyRouter>
where
    H: Handler<T>,
{
    on(MethodFilter::Head, handler)
}

/// Route `OPTIONS` requests to the given handler.
///
/// See [`get`] for an example.
pub fn options<H, T>(handler: H) -> OnMethod<IntoService<H, T>, EmptyRouter>
where
    H: Handler<T>,
{
    on(MethodFilter::Options, handler)
}

/// Route `PATCH` requests to the given handler.
///
/// See [`get`] for an example.
pub fn patch<H, T>(handler: H) -> OnMethod<IntoService<H, T>, EmptyRouter>
where
    H: Handler<T>,
{
    on(MethodFilter::Patch, handler)
}

/// Route `POST` requests to the given handler.
///
/// See [`get`] for an example.
pub fn post<H, T>(handler: H) -> OnMethod<IntoService<H, T>, EmptyRouter>
where
    H: Handler<T>,
{
    on(MethodFilter::Post, handler)
}

/// Route `PUT` requests to the given handler.
///
/// See [`get`] for an example.
pub fn put<H, T>(handler: H) -> OnMethod<IntoService<H, T>, EmptyRouter>
where
    H: Handler<T>,
{
    on(MethodFilter::Put, handler)
}

/// Route `TRACE` requests to the given handler.
///
/// See [`get`] for an example.
pub fn trace<H, T>(handler: H) -> OnMethod<IntoService<H, T>, EmptyRouter>
where
    H: Handler<T>,
{
    on(MethodFilter::Trace, handler)
}

/// Route requests with the given method to the handler.
///
/// # Example
///
/// ```rust
/// use tower_web::{handler::on, routing::MethodFilter, prelude::*};
///
/// async fn handler(request: Request<Body>) {}
///
/// // Requests to `POST /` will go to `handler`.
/// let app = route("/", on(MethodFilter::Post, handler));
/// ```
pub fn on<H, T>(method: MethodFilter, handler: H) -> OnMethod<IntoService<H, T>, EmptyRouter>
where
    H: Handler<T>,
{
    OnMethod {
        method,
        svc: handler.into_service(),
        fallback: EmptyRouter,
    }
}

mod sealed {
    #![allow(unreachable_pub, missing_docs, missing_debug_implementations)]

    pub trait HiddentTrait {}
    pub struct Hidden;
    impl HiddentTrait for Hidden {}
}

/// Trait for async functions that can be used to handle requests.
///
/// You shouldn't need to depend on this trait directly. It is automatically
/// implemented to closures of the right types.
///
/// # Example
///
/// Some examples of handlers:
///
/// ```rust
/// use tower_web::prelude::*;
/// use bytes::Bytes;
/// use http::StatusCode;
///
/// // Handlers must take `Request<Body>` as the first argument and must return
/// // something that implements `IntoResponse`, which `()` does
/// async fn unit_handler(request: Request<Body>) {}
///
/// // `String` also implements `IntoResponse`
/// async fn string_handler(request: Request<Body>) -> String {
///     "Hello, World!".to_string()
/// }
///
/// // Handler the buffers the request body and returns it if it is valid UTF-8
/// async fn buffer_body(request: Request<Body>, body: Bytes) -> Result<String, StatusCode> {
///     if let Ok(string) = String::from_utf8(body.to_vec()) {
///         Ok(string)
///     } else {
///         Err(StatusCode::BAD_REQUEST)
///     }
/// }
/// ```
///
/// For more details on generating responses see the
/// [`response`](crate::response) module and for more details on extractors see
/// the [`extract`](crate::extract) module.
#[async_trait]
pub trait Handler<In>: Sized {
    // This seals the trait. We cannot use the regular "sealed super trait" approach
    // due to coherence.
    #[doc(hidden)]
    type Sealed: sealed::HiddentTrait;

    /// Call the handler with the given request.
    async fn call(self, req: Request<Body>) -> Response<BoxBody>;

    /// Apply a [`tower::Layer`] to the handler.
    ///
    /// # Example
    ///
    /// Adding the [`tower::limit::ConcurrencyLimit`] middleware to a handler
    /// can be done with [`tower::limit::ConcurrencyLimitLayer`]:
    ///
    /// ```rust
    /// use tower_web::prelude::*;
    /// use tower::limit::{ConcurrencyLimitLayer, ConcurrencyLimit};
    ///
    /// async fn handler(request: Request<Body>) { /* ... */ }
    ///
    /// let layered_handler = handler.layer(ConcurrencyLimitLayer::new(64));
    /// ```
    ///
    /// When adding middleware that might fail its required to handle those
    /// errors. See [`Layered::handle_error`] for more details.
    fn layer<L>(self, layer: L) -> Layered<L::Service, In>
    where
        L: Layer<IntoService<Self, In>>,
    {
        Layered::new(layer.layer(IntoService::new(self)))
    }

    /// Convert the handler into a [`Service`].
    fn into_service(self) -> IntoService<Self, In> {
        IntoService::new(self)
    }
}

#[async_trait]
impl<F, Fut, Res> Handler<()> for F
where
    F: FnOnce(Request<Body>) -> Fut + Send + Sync,
    Fut: Future<Output = Res> + Send,
    Res: IntoResponse,
{
    type Sealed = sealed::Hidden;

    async fn call(self, req: Request<Body>) -> Response<BoxBody> {
        self(req).await.into_response().map(BoxBody::new)
    }
}

macro_rules! impl_handler {
    () => {};

    ( $head:ident, $($tail:ident),* $(,)? ) => {
        #[async_trait]
        #[allow(non_snake_case)]
        impl<F, Fut, Res, $head, $($tail,)*> Handler<($head, $($tail,)*)> for F
        where
            F: FnOnce(Request<Body>, $head, $($tail,)*) -> Fut + Send + Sync,
            Fut: Future<Output = Res> + Send,
            Res: IntoResponse,
            $head: FromRequest + Send,
            $( $tail: FromRequest + Send, )*
        {
            type Sealed = sealed::Hidden;

            async fn call(self, mut req: Request<Body>) -> Response<BoxBody> {
                let $head = match $head::from_request(&mut req).await {
                    Ok(value) => value,
                    Err(rejection) => return rejection.into_response().map(BoxBody::new),
                };

                $(
                    let $tail = match $tail::from_request(&mut req).await {
                        Ok(value) => value,
                        Err(rejection) => return rejection.into_response().map(BoxBody::new),
                    };
                )*

                let res = self(req, $head, $($tail,)*).await;

                res.into_response().map(BoxBody::new)
            }
        }

        impl_handler!($($tail,)*);
    };
}

impl_handler!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16);

/// A [`Service`] created from a [`Handler`] by applying a Tower middleware.
///
/// Created with [`Handler::layer`].
pub struct Layered<S, T> {
    svc: S,
    _input: PhantomData<fn() -> T>,
}

impl<S, T> fmt::Debug for Layered<S, T>
where
    S: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Layered").field("svc", &self.svc).finish()
    }
}

impl<S, T> Clone for Layered<S, T>
where
    S: Clone,
{
    fn clone(&self) -> Self {
        Self::new(self.svc.clone())
    }
}

#[async_trait]
impl<S, T, B> Handler<T> for Layered<S, T>
where
    S: Service<Request<Body>, Response = Response<B>> + Send,
    // S::Response: IntoResponse,
    S::Error: IntoResponse,
    S::Future: Send,
    B: http_body::Body<Data = Bytes> + Send + Sync + 'static,
    B::Error: Into<BoxError> + Send + Sync + 'static,
{
    type Sealed = sealed::Hidden;

    async fn call(self, req: Request<Body>) -> Response<BoxBody> {
        match self
            .svc
            .oneshot(req)
            .await
            .map_err(IntoResponse::into_response)
        {
            Ok(res) => res.map(BoxBody::new),
            Err(res) => res.map(BoxBody::new),
        }
    }
}

impl<S, T> Layered<S, T> {
    pub(crate) fn new(svc: S) -> Self {
        Self {
            svc,
            _input: PhantomData,
        }
    }

    /// Create a new [`Layered`] handler where errors will be handled using the
    /// given closure.
    ///
    /// tower-web requires that services gracefully handles all errors. That
    /// means when you apply a Tower middleware that adds a new failure
    /// condition you have to handle that as well.
    ///
    /// That can be done using `handle_error` like so:
    ///
    /// ```rust
    /// use tower_web::prelude::*;
    /// use http::StatusCode;
    /// use tower::{BoxError, timeout::TimeoutLayer};
    /// use std::time::Duration;
    ///
    /// async fn handler(request: Request<Body>) { /* ... */ }
    ///
    /// // `Timeout` will fail with `BoxError` if the timeout elapses...
    /// let layered_handler = handler
    ///     .layer(TimeoutLayer::new(Duration::from_secs(30)));
    ///
    /// // ...so we must handle that error
    /// let layered_handler = layered_handler.handle_error(|error: BoxError| {
    ///     if error.is::<tower::timeout::error::Elapsed>() {
    ///         (
    ///             StatusCode::REQUEST_TIMEOUT,
    ///             "request took too long".to_string(),
    ///         )
    ///     } else {
    ///         (
    ///             StatusCode::INTERNAL_SERVER_ERROR,
    ///             format!("Unhandled internal error: {}", error),
    ///         )
    ///     }
    /// });
    /// ```
    ///
    /// The closure can return any type that implements [`IntoResponse`].
    pub fn handle_error<F, B, Res>(self, f: F) -> Layered<HandleError<S, F>, T>
    where
        S: Service<Request<Body>, Response = Response<B>>,
        F: FnOnce(S::Error) -> Res,
        Res: IntoResponse,
    {
        let svc = HandleError::new(self.svc, f);
        Layered::new(svc)
    }
}

/// An adapter that makes a [`Handler`] into a [`Service`].
///
/// Created with [`Handler::into_service`].
pub struct IntoService<H, T> {
    handler: H,
    _marker: PhantomData<fn() -> T>,
}

impl<H, T> IntoService<H, T> {
    fn new(handler: H) -> Self {
        Self {
            handler,
            _marker: PhantomData,
        }
    }
}

impl<H, T> fmt::Debug for IntoService<H, T>
where
    H: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IntoService")
            .field("handler", &self.handler)
            .finish()
    }
}

impl<H, T> Clone for IntoService<H, T>
where
    H: Clone,
{
    fn clone(&self) -> Self {
        Self {
            handler: self.handler.clone(),
            _marker: PhantomData,
        }
    }
}

impl<H, T> Service<Request<Body>> for IntoService<H, T>
where
    H: Handler<T> + Clone + Send + 'static,
{
    type Response = Response<BoxBody>;
    type Error = Infallible;
    type Future = IntoServiceFuture;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        // `IntoService` can only be constructed from async functions which are always ready, or from
        // `Layered` which bufferes in `<Layered as Handler>::call` and is therefore also always
        // ready.
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        let handler = self.handler.clone();
        let future = Box::pin(async move {
            let res = Handler::call(handler, req).await;
            Ok(res)
        });
        IntoServiceFuture(future)
    }
}

opaque_future! {
    /// The response future for [`IntoService`].
    pub type IntoServiceFuture =
        future::BoxFuture<'static, Result<Response<BoxBody>, Infallible>>;
}

/// A handler [`Service`] that accepts requests based on a [`MethodFilter`].
#[derive(Debug, Clone, Copy)]
pub struct OnMethod<S, F> {
    pub(crate) method: MethodFilter,
    pub(crate) svc: S,
    pub(crate) fallback: F,
}

impl<S, F> OnMethod<S, F> {
    /// Chain an additional handler that will accept all requests regardless of
    /// its HTTP method.
    ///
    /// See [`OnMethod::get`] for an example.
    pub fn any<H, T>(self, handler: H) -> OnMethod<IntoService<H, T>, Self>
    where
        H: Handler<T>,
    {
        self.on(MethodFilter::Any, handler)
    }

    /// Chain an additional handler that will only accept `CONNECT` requests.
    ///
    /// See [`OnMethod::get`] for an example.
    pub fn connect<H, T>(self, handler: H) -> OnMethod<IntoService<H, T>, Self>
    where
        H: Handler<T>,
    {
        self.on(MethodFilter::Connect, handler)
    }

    /// Chain an additional handler that will only accept `DELETE` requests.
    ///
    /// See [`OnMethod::get`] for an example.
    pub fn delete<H, T>(self, handler: H) -> OnMethod<IntoService<H, T>, Self>
    where
        H: Handler<T>,
    {
        self.on(MethodFilter::Delete, handler)
    }

    /// Chain an additional handler that will only accept `GET` requests.
    ///
    /// # Example
    ///
    /// ```rust
    /// use tower_web::prelude::*;
    ///
    /// async fn handler(request: Request<Body>) {}
    ///
    /// async fn other_handler(request: Request<Body>) {}
    ///
    /// // Requests to `GET /` will go to `handler` and `POST /` will go to
    /// // `other_handler`.
    /// let app = route("/", post(handler).get(other_handler));
    /// ```
    pub fn get<H, T>(self, handler: H) -> OnMethod<IntoService<H, T>, Self>
    where
        H: Handler<T>,
    {
        self.on(MethodFilter::Get, handler)
    }

    /// Chain an additional handler that will only accept `HEAD` requests.
    ///
    /// See [`OnMethod::get`] for an example.
    pub fn head<H, T>(self, handler: H) -> OnMethod<IntoService<H, T>, Self>
    where
        H: Handler<T>,
    {
        self.on(MethodFilter::Head, handler)
    }

    /// Chain an additional handler that will only accept `OPTIONS` requests.
    ///
    /// See [`OnMethod::get`] for an example.
    pub fn options<H, T>(self, handler: H) -> OnMethod<IntoService<H, T>, Self>
    where
        H: Handler<T>,
    {
        self.on(MethodFilter::Options, handler)
    }

    /// Chain an additional handler that will only accept `PATCH` requests.
    ///
    /// See [`OnMethod::get`] for an example.
    pub fn patch<H, T>(self, handler: H) -> OnMethod<IntoService<H, T>, Self>
    where
        H: Handler<T>,
    {
        self.on(MethodFilter::Patch, handler)
    }

    /// Chain an additional handler that will only accept `POST` requests.
    ///
    /// See [`OnMethod::get`] for an example.
    pub fn post<H, T>(self, handler: H) -> OnMethod<IntoService<H, T>, Self>
    where
        H: Handler<T>,
    {
        self.on(MethodFilter::Post, handler)
    }

    /// Chain an additional handler that will only accept `PUT` requests.
    ///
    /// See [`OnMethod::get`] for an example.
    pub fn put<H, T>(self, handler: H) -> OnMethod<IntoService<H, T>, Self>
    where
        H: Handler<T>,
    {
        self.on(MethodFilter::Put, handler)
    }

    /// Chain an additional handler that will only accept `TRACE` requests.
    ///
    /// See [`OnMethod::get`] for an example.
    pub fn trace<H, T>(self, handler: H) -> OnMethod<IntoService<H, T>, Self>
    where
        H: Handler<T>,
    {
        self.on(MethodFilter::Trace, handler)
    }

    /// Chain an additional handler that will accept requests matching the given
    /// `MethodFilter`.
    ///
    /// # Example
    ///
    /// ```rust
    /// use tower_web::{routing::MethodFilter, prelude::*};
    ///
    /// async fn handler(request: Request<Body>) {}
    ///
    /// async fn other_handler(request: Request<Body>) {}
    ///
    /// // Requests to `GET /` will go to `handler` and `DELETE /` will go to
    /// // `other_handler`
    /// let app = route("/", get(handler).on(MethodFilter::Delete, other_handler));
    /// ```
    pub fn on<H, T>(self, method: MethodFilter, handler: H) -> OnMethod<IntoService<H, T>, Self>
    where
        H: Handler<T>,
    {
        OnMethod {
            method,
            svc: handler.into_service(),
            fallback: self,
        }
    }
}

impl<S, F, SB, FB> Service<Request<Body>> for OnMethod<S, F>
where
    S: Service<Request<Body>, Response = Response<SB>, Error = Infallible> + Clone,
    SB: http_body::Body<Data = Bytes> + Send + Sync + 'static,
    SB::Error: Into<BoxError>,

    F: Service<Request<Body>, Response = Response<FB>, Error = Infallible> + Clone,
    FB: http_body::Body<Data = Bytes> + Send + Sync + 'static,
    FB::Error: Into<BoxError>,
{
    type Response = Response<BoxBody>;
    type Error = Infallible;
    type Future = RouteFuture<S, F>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        let f = if self.method.matches(req.method()) {
            let response_future = self.svc.clone().oneshot(req);
            future::Either::Left(BoxResponseBody(response_future))
        } else {
            let response_future = self.fallback.clone().oneshot(req);
            future::Either::Right(BoxResponseBody(response_future))
        };
        RouteFuture(f)
    }
}
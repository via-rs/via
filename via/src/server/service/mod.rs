mod upgrade;

pub use upgrade::{Supervisor, Upgraded, upgrade};

use hyper::body::Incoming;
use hyper::service::Service;
use std::collections::VecDeque;
use std::convert::Infallible;
use std::num::NonZeroUsize;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use super::cancel::CancellationToken;
use crate::app::{Shared, Via};
use crate::request::{Envelope, Request, RequestBody};
use crate::response::ResponseBody;
use crate::{BoxFuture, Error, Next, Router, deny, err};

#[cfg(feature = "test-util")]
use crate::test::TestBody;

use upgrade::upgradeable;

const MAX_URI_PATH_LEN: usize = 8092; // 8 KB

#[cfg(feature = "test-util")]
type ServiceRequest = http::Request<TestBody>;

#[cfg(not(feature = "test-util"))]
type ServiceRequest = http::Request<Incoming>;

pub(crate) struct FutureResponse {
    future: BoxFuture,
}

pub(crate) struct ServiceAdapter<App> {
    service: Arc<ViaService<App>>,
}

struct ViaService<App> {
    max_request_size: NonZeroUsize,
    cancellation: CancellationToken,
    router: Router<App>,
    app: Shared<App>,
}

impl FutureResponse {
    fn max_path_len_exceeded() -> Self {
        let future = Box::pin(async {
            Err(err!(
                414,
                "path exceeds the maximum allowed length of 8 kb."
            ))
        });

        Self { future }
    }
}

impl Future for FutureResponse {
    type Output = Result<http::Response<ResponseBody>, Infallible>;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        // Get the mutable reference contained in `Pin<&mut Self>`. If we were
        // to rely on the blanket DerefMut impl for `Unpin` types, we would
        // perform reification twice unnecessarily. Reification is the
        // responsibility of the owner of the allocation.
        let this = self.get_mut();

        // Calling `as_mut` on a `BoxFuture` reifies the borrow and it is a
        // prerequisite when polling a `Pin<Box<_>>` from a `Pin<&mut _>`.
        let future = this.future.as_mut();

        if let Poll::Ready(result) = future.poll(context) {
            // If an error originates in a service, convert it to a response.
            let response = result.unwrap_or_else(|error| error.into());

            // Unwrap the `http::Response` from the `via::Response`.
            Poll::Ready(Ok(response.into()))
        } else {
            Poll::Pending
        }
    }
}

impl<App> ServiceAdapter<App> {
    pub(crate) fn new(max_request_size: usize, via: Via<App>) -> Result<Self, Error> {
        let Some(max_request_size) = NonZeroUsize::new(max_request_size) else {
            deny!(500, "max_request_size must be > 0.");
        };

        let (router, app) = via.into_parts();
        let cancellation = CancellationToken::new();

        Ok(Self {
            service: Arc::new(ViaService {
                max_request_size,
                cancellation,
                router,
                app,
            }),
        })
    }

    pub(super) fn cancellation(&self) -> &CancellationToken {
        &self.service().cancellation
    }

    fn service(&self) -> &ViaService<App> {
        &self.service
    }
}

impl<App> Clone for ServiceAdapter<App> {
    fn clone(&self) -> Self {
        Self {
            service: Arc::clone(&self.service),
        }
    }
}

impl<App> Service<ServiceRequest> for ServiceAdapter<App> {
    type Error = Infallible;
    type Future = FutureResponse;
    type Response = http::Response<ResponseBody>;

    #[inline(never)]
    fn call(&self, request: ServiceRequest) -> Self::Future {
        self.service().call(request)
    }
}

#[cfg(feature = "test-util")]
impl<App> ServiceAdapter<App> {
    pub fn app(&self) -> &Shared<App> {
        &self.service().app
    }
}

#[cfg(feature = "test-util")]
impl<App> Service<http::Request<Incoming>> for ServiceAdapter<App> {
    type Error = Infallible;
    type Future = FutureResponse;
    type Response = http::Response<ResponseBody>;

    fn call(&self, request: http::Request<Incoming>) -> Self::Future {
        self.service().call(request.map(TestBody::new))
    }
}

impl<App> Service<ServiceRequest> for ViaService<App> {
    type Error = Infallible;
    type Future = FutureResponse;
    type Response = http::Response<ResponseBody>;

    #[inline(never)]
    fn call(&self, mut request: ServiceRequest) -> Self::Future {
        let path = request.uri().path();

        // Immediately respond with 414 if the path length exceeds the maximum.
        if path.len() > MAX_URI_PATH_LEN {
            FutureResponse::max_path_len_exceeded()
        } else {
            // The middleware stack.
            let mut deque = VecDeque::with_capacity(18);

            // Preallocate enough space to store at least 6 path params.
            let mut params = Vec::with_capacity(6);

            // Preallocate enough space to store 9 frames of request body data.
            let frames = Vec::with_capacity(9);

            // Populate the middleware stack with the resolved routes.
            for (route, param) in self.router.traverse(path) {
                // Extend deque with the route's middleware stack.
                deque.extend(route);

                // Extend params with the route's optional dynamic parameter.
                params.extend(param);
            }

            // Insert an upgrade supervisor into the request extensions.
            upgradeable(&mut request, self.cancellation.clone());

            // Wrap the incoming request with our custom Request struct.
            let (parts, body) = request.into_parts();
            let request = Request::new(
                Envelope::new(parts, params),
                // Limit request body sizes to the configured maximum.
                RequestBody::new(self.max_request_size.get(), body, frames),
                // Request owns a copy of Shared<App>.
                self.app.clone(),
            );

            // Call the middleware stack to get a response.
            FutureResponse {
                future: Next::new(deque).call(request),
            }
        }
    }
}

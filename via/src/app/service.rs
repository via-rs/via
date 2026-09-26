use delegate::delegate;
use hyper::body::Incoming;
use hyper::service::Service;
use std::collections::VecDeque;
use std::convert::Infallible;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use super::{Shared, Via};
use crate::request::{Envelope, Request, RequestBody};
use crate::response::ResponseBody;
use crate::server::{ServerConfig, UpgradeSupervisor};
use crate::{BoxFuture, Next, Router, err};

#[cfg(feature = "test-util")]
use crate::test::TestBody;

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

pub(crate) struct ConnectionService<App> {
    service: UpgradeableService<App>,
}

struct UpgradeableService<App> {
    service: Arc<ViaService<App>>,
    upgrade: UpgradeSupervisor,
}

struct ViaService<App> {
    config: Box<ServerConfig>,
    via: Via<App>,
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

impl<App> ConnectionService<App> {
    #[inline]
    pub(crate) fn config(&self) -> &ServerConfig {
        self.service.config()
    }

    #[inline(always)]
    pub(crate) fn supervisor(&self) -> &UpgradeSupervisor {
        &self.service.upgrade
    }
}

impl<App> Service<ServiceRequest> for ConnectionService<App> {
    type Error = Infallible;
    type Future = FutureResponse;
    type Response = http::Response<ResponseBody>;

    #[inline(never)]
    fn call(&self, request: ServiceRequest) -> Self::Future {
        self.service.call(request)
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
    pub(crate) fn new(config: ServerConfig, via: Via<App>) -> Self {
        let config = Box::new(config);

        Self {
            service: Arc::new(ViaService { config, via }),
        }
    }

    #[inline]
    pub(crate) fn config(&self) -> &ServerConfig {
        self.service().config()
    }

    #[inline]
    pub(crate) fn into_service(self) -> ConnectionService<App> {
        ConnectionService {
            service: UpgradeableService {
                service: self.service,
                upgrade: UpgradeSupervisor::new(),
            },
        }
    }

    #[cfg(feature = "test-util")]
    pub(crate) fn app(&self) -> &Shared<App> {
        self.service.via.app()
    }

    fn service(&self) -> &ViaService<App> {
        &self.service
    }
}

impl<App> Clone for ServiceAdapter<App> {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            service: Arc::clone(&self.service),
        }
    }
}

#[cfg(feature = "test-util")]
impl<App> Service<http::Request<Incoming>> for ConnectionService<App> {
    type Error = Infallible;
    type Future = FutureResponse;
    type Response = http::Response<ResponseBody>;

    fn call(&self, request: http::Request<Incoming>) -> Self::Future {
        self.service.call(request.map(TestBody::new))
    }
}

impl<App> UpgradeableService<App> {
    delegate! {
        to self.service() {
            fn config(&self) -> &ServerConfig;
        }
    }

    fn service(&self) -> &ViaService<App> {
        &self.service
    }

    fn supervisor(&self) -> &UpgradeSupervisor {
        &self.upgrade
    }
}

impl<App> Service<ServiceRequest> for UpgradeableService<App> {
    type Error = Infallible;
    type Future = FutureResponse;
    type Response = http::Response<ResponseBody>;

    #[inline(never)]
    fn call(&self, request: ServiceRequest) -> Self::Future {
        let path = request.uri().path();

        // Immediately respond with 414 if the path length exceeds the maximum.
        if path.len() > MAX_URI_PATH_LEN {
            FutureResponse::max_path_len_exceeded()
        } else {
            let service = self.service();

            // The middleware stack.
            let mut deque = VecDeque::with_capacity(18);

            // Preallocate enough space to store at least 6 path params.
            let mut params = Vec::with_capacity(6);

            // Populate the middleware stack with the resolved routes.
            for (route, param) in service.router().traverse(path) {
                // Extend deque with the route's middleware stack.
                deque.extend(route);

                // Extend params with the route's optional dynamic parameter.
                params.extend(param);
            }

            // Wrap the incoming request with our custom Request struct.
            let mut request = {
                let (parts, body) = request.into_parts();

                // Params are stored adjacent to the request head. This allows us
                // to discard the body and drop the associated channel if the
                // request is upgraded and moved into a WebSocket task.
                let envelope = Envelope::new(parts, params);

                // Preallocate enough space to store 9 frames of request body data.
                let frames = Vec::with_capacity(9);

                // Limit request body sizes to the configured maximum.
                let body = RequestBody::new(service.config().max_request_size(), body, frames);

                // Request owns a copy of Shared<App>.
                let app = service.app().clone();

                Request::new(envelope, body, app)
            };

            // Insert a supervisor into the request extensions.
            //
            // It will *eventually* contain a panic handle to shutdown the
            // server if a panic occurs in a websocket reactor task.
            let supervisor = self.supervisor().clone();

            if request.extensions_mut().insert(supervisor).is_none() {
                // Placeholder for tracing...
            }

            // Call the middleware stack to get a response.
            FutureResponse {
                future: Next::new(deque).call(request),
            }
        }
    }
}

impl<App> ViaService<App> {
    delegate! {
        to self.via {
            fn app(&self) -> &Shared<App>;
            fn router(&self) -> &Router<App>;
        }
    }

    fn config(&self) -> &ServerConfig {
        &self.config
    }
}

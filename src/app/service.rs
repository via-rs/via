use http_body_util::Limited;
use hyper::body::Incoming;
use hyper::service::Service;
use std::collections::VecDeque;
use std::convert::Infallible;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use crate::request::Envelope;
use crate::response::{Response, ResponseBody};
use crate::server::ServerConfig;
use crate::{BoxFuture, Next, Request, Via, raise};

const MAX_URI_PATH_LEN: usize = 8092; // 8 KB

pub struct FutureResponse(BoxFuture);

pub struct ServiceAdapter<App> {
    service: Arc<ViaService<App>>,
}

struct ViaService<App> {
    config: Box<ServerConfig>,
    via: Via<App>,
}

impl FutureResponse {
    fn max_path_len_exceeded() -> Self {
        Self(Box::pin(async {
            raise!(
                414,
                message = "path exceeds the maximum allowed length of 8 KB",
            );
        }))
    }
}

impl Future for FutureResponse {
    type Output = Result<http::Response<ResponseBody>, Infallible>;

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        self.0
            .as_mut()
            .poll(context)
            .map(|result| Ok(result.unwrap_or_else(Response::from).into()))
    }
}

impl<App> ServiceAdapter<App> {
    pub(crate) fn new(config: ServerConfig, via: Via<App>) -> Self {
        let config = Box::new(config);

        Self {
            service: Arc::new(ViaService { config, via }),
        }
    }

    pub(crate) fn config(&self) -> &ServerConfig {
        &self.service.config
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

impl<App> Service<http::Request<Incoming>> for ServiceAdapter<App> {
    type Error = Infallible;
    type Future = FutureResponse;
    type Response = http::Response<ResponseBody>;

    fn call(&self, request: http::Request<Incoming>) -> Self::Future {
        self.service.call(request)
    }
}

impl<App> Service<http::Request<Incoming>> for ViaService<App> {
    type Error = Infallible;
    type Future = FutureResponse;
    type Response = http::Response<ResponseBody>;

    fn call(&self, request: http::Request<Incoming>) -> Self::Future {
        let path = request.uri().path();

        // Immediately respond with 414 if the path length exceeds the maximum.
        if path.len() > MAX_URI_PATH_LEN {
            return FutureResponse::max_path_len_exceeded();
        }

        // The middleware stack.
        let mut deque = VecDeque::with_capacity(18);

        // Preallocate enough space to store at least 6 path params.
        let mut params = Vec::with_capacity(6);

        // Populate the middleware stack with the resolved routes.
        for (route, param) in self.via.router().traverse(path) {
            // Extend the deque with the route's middleware stack.
            deque.extend(route.cloned());

            if let Some((name, range)) = param {
                // Include the route's dynamic parameter in params.
                params.push((name.clone(), [Some(range.0), range.1]));
            }
        }

        // Request owns a copy of Shared<App>.
        let app = self.via.app().clone();

        // Wrap the incoming request with our custom Request struct.
        let request = {
            let (parts, body) = request.into_parts();

            // Params are stored adjacent to the request head. This allows us
            // to discard the body and drop the associated channel if the
            // request is upgraded and moved into a WebSocket task.
            let envelope = Envelope::new(parts, params);

            // Limit request body sizes to the configured maximum.
            let body = Limited::new(body, self.config.max_request_size());

            Request::new(envelope, body, app)
        };

        // Call the middleware stack to get a response.
        FutureResponse(Next::new(deque).call(request))
    }
}

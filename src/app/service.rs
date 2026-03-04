use hyper::body::Incoming;
use hyper::service::Service;
use std::collections::VecDeque;
use std::convert::Infallible;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use crate::response::{Response, ResponseBody};
use crate::{BoxFuture, Next, Request, Via, raise};

const MAX_URI_PATH_LEN: usize = 8092;
const MAX_PATH_LEN_EXCEEDED: &str = "path exceeds the maximum allowed length of 8 KB";

pub struct FutureResponse(BoxFuture);

pub struct AppService<App> {
    app: Arc<Via<App>>,
    max_request_size: usize,
}

impl<App> AppService<App> {
    #[inline]
    pub(crate) fn new(app: Arc<Via<App>>, max_request_size: usize) -> Self {
        Self {
            app,
            max_request_size,
        }
    }
}

impl<App> Clone for AppService<App> {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            app: Arc::clone(&self.app),
            max_request_size: self.max_request_size,
        }
    }
}

impl<App> Service<http::Request<Incoming>> for AppService<App> {
    type Error = Infallible;
    type Future = FutureResponse;
    type Response = http::Response<ResponseBody>;

    fn call(&self, request: http::Request<Incoming>) -> Self::Future {
        // Immediately respond with 414 if the path length exceeds the maximum.
        if request.uri().path().len() > MAX_URI_PATH_LEN {
            return FutureResponse(Box::pin(async {
                raise!(414, message = MAX_PATH_LEN_EXCEEDED);
            }));
        }

        // The middleware stack.
        let mut deque = VecDeque::with_capacity(18);

        // Wrap the raw HTTP request in our custom Request struct.
        let mut request = {
            // Preallocate enough space to store at least 6 path params.
            let params = Vec::with_capacity(6);

            // Request owns a copy of Shared<App>.
            let app = self.app.app.clone();

            Request::new(app, self.max_request_size, params, request)
        };

        // Get a mutable ref to params and a shared ref to the uri path.
        let (params, path) = request.envelope_mut().params_mut_with_path();

        // Populate the middleware stack with the resolved routes.
        for (route, param) in self.app.router.traverse(path) {
            // Extend the deque with the route's middleware stack.
            deque.extend(route.cloned());

            if let Some((name, range)) = param {
                // Include the route's dynamic parameter in params.
                params.push((name.clone(), [Some(range.0), range.1]));
            }
        }

        // Call the middleware stack to get a response.
        FutureResponse(Next::new(deque).call(request))
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

use bytes::Bytes;
use http_body::{Body, Frame, SizeHint};
use http_body_util::BodyExt;
use hyper::service::Service;
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::app::{ServiceAdapter, Via};
use crate::response::ResponseBody;
use crate::{Error, Router};

#[derive(Debug, Default)]
pub struct TestBody(ResponseBody);

pub trait Client {
    #[allow(async_fn_in_trait)]
    async fn send(&self, request: http::Request<TestBody>) -> http::Response<ResponseBody>;
}

pub struct TestService<App> {
    service: ServiceAdapter<App>,
}

pub fn service<App>(router: Router<App>, app: App) -> TestService<App> {
    let via = Via::new(router, app);
    let config = Default::default();

    TestService {
        service: ServiceAdapter::new(config, via),
    }
}

impl<App> Client for TestService<App> {
    async fn send(&self, request: http::Request<TestBody>) -> http::Response<ResponseBody> {
        match self.service.call(request).await {
            Ok(response) => response,
            Err(_) => unreachable!(),
        }
    }
}

impl TestBody {
    pub fn new<T>(body: T) -> Self
    where
        T: Body<Data = Bytes> + Send + Sync + 'static,
        Error: From<T::Error>,
    {
        Self(ResponseBody::boxed(body.map_err(Error::from)))
    }
}

impl Body for TestBody {
    type Data = Bytes;
    type Error = Error;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        context: &mut Context,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        Pin::new(&mut self.0).poll_frame(context)
    }

    fn is_end_stream(&self) -> bool {
        self.0.is_end_stream()
    }

    fn size_hint(&self) -> SizeHint {
        self.0.size_hint()
    }
}

use bytes::Bytes;
use http::request::Builder;
use http::{HeaderName, HeaderValue, Method, Uri, Version};
use http_body::{Body, Frame, SizeHint};
use http_body_util::BodyExt;
use http_body_util::combinators::BoxBody;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};

use super::service::Client;
use crate::{Error, Request, Response};

pub struct RequestBuilder<App> {
    request: Builder,
    body: Option<TestBody>,
    _app: PhantomData<App>,
}

#[derive(Debug, Default)]
pub struct TestBody {
    body: BoxBody<Bytes, Error>,
}

macro_rules! methods {
    ($($vis:vis fn $name:ident($method:ident));+ $(;)?) => {
        $(
            #[doc = concat!(
                "Build a request with method `",
                stringify!($method),
                "` to the provided `uri`.",
            )]
            $vis fn $name<T>(uri: T) -> RequestBuilder<App>
            where
                T: TryInto<Uri>,
                <T as TryInto<Uri>>::Error: Into<http::Error>,
            {
                RequestBuilder {
                    request: http::request::Request::$name(uri),
                    body: None,
                    _app: PhantomData,
                }
            }
        )+
    };
}

impl<App> Request<App> {
    methods! {
        pub fn connect(CONNECT);
        pub fn delete(DELETE);
        pub fn get(GET);
        pub fn head(HEAD);
        pub fn options(OPTIONS);
        pub fn patch(PATCH);
        pub fn post(POST);
        pub fn put(PUT);
        pub fn trace(TRACE);
    }
}

impl<App> RequestBuilder<App> {
    pub fn method<T>(mut self, method: T) -> Self
    where
        T: TryInto<Method>,
        <T as TryInto<Method>>::Error: Into<http::Error>,
    {
        self.request = self.request.method(method);
        self
    }

    pub fn uri<T>(mut self, uri: T) -> Self
    where
        T: TryInto<Uri>,
        <T as TryInto<Uri>>::Error: Into<http::Error>,
    {
        self.request = self.request.uri(uri);
        self
    }

    pub fn version(mut self, version: Version) -> Self {
        self.request = self.request.version(version);
        self
    }

    pub fn header<K, V>(mut self, key: K, value: V) -> Self
    where
        HeaderName: TryFrom<K>,
        <HeaderName as TryFrom<K>>::Error: Into<http::Error>,
        HeaderValue: TryFrom<V>,
        <HeaderValue as TryFrom<V>>::Error: Into<http::Error>,
    {
        self.request = self.request.header(key, value);
        self
    }

    pub fn extension<T>(mut self, extension: T) -> Self
    where
        T: Clone + Send + Sync + 'static,
    {
        self.request = self.request.extension(extension);
        self
    }

    pub fn body<T>(mut self, body: T) -> Self
    where
        T: Body<Data = Bytes> + Send + Sync + 'static,
        Error: From<T::Error>,
    {
        self.body = Some(TestBody::new(body));
        self
    }

    pub async fn send(self, client: &impl Client<App>) -> Result<Response, Error> {
        let body = self.body.unwrap_or_default();
        let request = self.request.body(body)?;

        client.send(request).await
    }
}

impl TestBody {
    pub(crate) fn new<T>(body: T) -> Self
    where
        T: Body<Data = Bytes> + Send + Sync + 'static,
        Error: From<T::Error>,
    {
        Self {
            body: BoxBody::new(body.map_err(Error::from)),
        }
    }
}

impl Body for TestBody {
    type Data = Bytes;
    type Error = Error;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        context: &mut Context,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        Pin::new(&mut self.body).poll_frame(context)
    }

    fn is_end_stream(&self) -> bool {
        self.body.is_end_stream()
    }

    fn size_hint(&self) -> SizeHint {
        self.body.size_hint()
    }
}

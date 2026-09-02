use bytes::Bytes;
use http::header::{CONTENT_LENGTH, CONTENT_TYPE};
use http::request::Builder;
use http::{HeaderName, HeaderValue, Method, StatusCode, Uri, Version};
use http_body::{Body, Frame, SizeHint};
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Full};
use serde::Serialize;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};

use super::client::Client;
use crate::error::{BoxError, Error};
use crate::request::Request;
use crate::response::Response;

pub struct RequestBuilder<App> {
    request: Builder,
    body: Result<Option<TestBody>, Error>,
    _app: PhantomData<App>,
}

#[derive(Debug, Default)]
pub struct TestBody {
    body: BoxBody<Bytes, BoxError>,
}

#[derive(Serialize)]
struct JsonData<T> {
    data: T,
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
                    body: Ok(None),
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
        TestBody: From<T>,
    {
        self.body = Ok(Some(body.into()));
        self
    }

    pub fn data<T>(self, data: T) -> Self
    where
        T: Serialize,
    {
        self.json(&JsonData { data })
    }

    pub fn json(mut self, body: &impl Serialize) -> Self {
        match serde_json::to_vec(body) {
            Ok(body) => self
                .header(CONTENT_LENGTH, body.len())
                .header(CONTENT_TYPE, "application/json; charset=utf-8")
                .body(body),

            Err(error) => {
                self.body = Err(Error::from_serde_json(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    error,
                ));

                self
            }
        }
    }

    pub async fn send(self, client: &mut impl Client<App>) -> Result<Response, Error> {
        let body = self.body?.unwrap_or_default();
        client.send(self.request.body(body)?).await
    }
}

impl TestBody {
    pub(crate) fn new<T>(body: T) -> Self
    where
        T: Body<Data = Bytes> + Send + Sync + 'static,
        BoxError: From<T::Error>,
    {
        Self {
            body: BoxBody::new(body.map_err(BoxError::from)),
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
        Pin::new(&mut self.body)
            .poll_frame(context)
            .map_err(Error::from_source)
    }

    fn is_end_stream(&self) -> bool {
        self.body.is_end_stream()
    }

    fn size_hint(&self) -> SizeHint {
        self.body.size_hint()
    }
}

impl<T> From<T> for TestBody
where
    Full<Bytes>: From<T>,
{
    fn from(body: T) -> Self {
        Self::new(Full::from(body))
    }
}

use http::{HeaderName, HeaderValue, Method, Uri, Version};

use crate::{Error, test::TestBody};

use super::Request;

macro_rules! methods {
    ($($vis:vis fn $name:ident($method:ident));+ $(;)?) => {
        $(
            #[doc = concat!(
                "Build a request with method `",
                stringify!($method),
                "` to the provided `uri`.",
            )]
            $vis fn $name<T>(uri: T) -> RequestBuilder
            where
                T: TryInto<Uri>,
                <T as TryInto<Uri>>::Error: Into<http::Error>,
            {
                RequestBuilder {
                    request: http::request::Request::$name(uri),
                }
            }
        )+
    };
}

#[derive(Default)]
pub struct RequestBuilder {
    request: http::request::Builder,
}

impl<App> Request<App> {
    pub fn builder() -> RequestBuilder {
        RequestBuilder::new()
    }

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

impl RequestBuilder {
    pub fn new() -> Self {
        Default::default()
    }

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

    pub fn body(self, body: TestBody) -> Result<http::Request<TestBody>, Error> {
        Ok(self.request.body(body)?)
    }

    /// Convert self into a [Response] with an empty payload.
    ///
    #[inline]
    pub fn finish(self) -> Result<http::Request<TestBody>, Error> {
        self.body(Default::default())
    }
}

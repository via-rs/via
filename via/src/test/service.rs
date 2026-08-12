use bytes::{Buf, Bytes};
use cookie::CookieJar;
use http::header::COOKIE;
use http::{HeaderMap, HeaderName, HeaderValue, StatusCode, Uri};
use http_body_util::BodyExt;
use hyper::service::Service;
use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::app::{ServiceAdapter, Shared, Via};
use crate::{Error, Request, Response, Router};

use super::request::TestBody;

pub trait Client<App>: Sized {
    fn send(&self, request: http::Request<TestBody>) -> impl Future<Output = crate::Result>;

    fn connect<T>(&self, uri: T) -> impl Future<Output = crate::Result>
    where
        T: TryInto<Uri>,
        <T as TryInto<Uri>>::Error: Into<http::Error>,
    {
        Request::<App>::connect(uri).send(self)
    }

    fn delete<T>(&self, uri: T) -> impl Future<Output = crate::Result>
    where
        T: TryInto<Uri>,
        <T as TryInto<Uri>>::Error: Into<http::Error>,
    {
        Request::<App>::delete(uri).send(self)
    }

    fn get<T>(&self, uri: T) -> impl Future<Output = crate::Result>
    where
        T: TryInto<Uri>,
        <T as TryInto<Uri>>::Error: Into<http::Error>,
    {
        Request::<App>::get(uri).send(self)
    }

    fn head<T>(&self, uri: T) -> impl Future<Output = crate::Result>
    where
        T: TryInto<Uri>,
        <T as TryInto<Uri>>::Error: Into<http::Error>,
    {
        Request::<App>::head(uri).send(self)
    }

    fn options<T>(&self, uri: T) -> impl Future<Output = crate::Result>
    where
        T: TryInto<Uri>,
        <T as TryInto<Uri>>::Error: Into<http::Error>,
    {
        Request::<App>::options(uri).send(self)
    }

    fn trace<T>(&self, uri: T) -> impl Future<Output = crate::Result>
    where
        T: TryInto<Uri>,
        <T as TryInto<Uri>>::Error: Into<http::Error>,
    {
        Request::<App>::trace(uri).send(self)
    }
}

pub struct TestService<App> {
    service: ServiceAdapter<App>,
    headers: HeaderMap,
    cookies: CookieJar,
}

pub fn service<App>(router: Router<App>, app: App) -> TestService<App> {
    let via = Via::new(router, app);
    let config = Default::default();

    TestService {
        service: ServiceAdapter::new(config, via),
        headers: HeaderMap::new(),
        cookies: CookieJar::new(),
    }
}

impl Response {
    pub async fn bytes(self) -> Result<Bytes, Error> {
        let payload = self.into_body().collect().await?;
        let mut buf = payload.aggregate();

        Ok(buf.copy_to_bytes(buf.remaining()))
    }

    pub async fn data<T>(self) -> Result<T, Error>
    where
        T: DeserializeOwned,
    {
        #[derive(Deserialize)]
        struct JsonData<T> {
            data: T,
        }

        self.json().await.map(|JsonData { data }| data)
    }

    pub async fn json<T>(self) -> Result<T, Error>
    where
        T: DeserializeOwned,
    {
        let bytes = self.bytes().await?;

        match serde_json::from_slice(bytes.as_ref()) {
            Ok(output) => Ok(output),
            Err(error) => Err(Error::from_serde_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                error,
            )),
        }
    }

    pub async fn text(self) -> Result<String, Error> {
        let bytes = self.bytes().await?;
        Ok(String::from_utf8(bytes.into())?)
    }
}

impl<App> TestService<App> {
    pub fn header<K, V>(mut self, key: K, value: V) -> Result<Self, Error>
    where
        HeaderName: TryFrom<K>,
        <HeaderName as TryFrom<K>>::Error: Into<http::Error>,
        HeaderValue: TryFrom<V>,
        <HeaderValue as TryFrom<V>>::Error: Into<http::Error>,
    {
        let key = HeaderName::try_from(key).map_err(Into::into)?;
        let value = HeaderValue::try_from(value).map_err(Into::into)?;

        self.headers.try_insert(key, value)?;

        Ok(self)
    }

    pub fn app(&self) -> &Shared<App> {
        self.service.app()
    }

    pub fn cookies_mut(&mut self) -> &mut CookieJar {
        &mut self.cookies
    }
}

impl<App> Client<App> for TestService<App> {
    fn send(&self, mut request: http::Request<TestBody>) -> impl Future<Output = crate::Result> {
        let service = self.service.clone();
        let headers = self.headers.clone();
        let cookies = self.cookies.iter().fold(String::new(), |value, cookie| {
            value + "; " + &cookie.to_string()
        });

        async move {
            // Append the default headers to the request.
            request.headers_mut().extend(headers);

            if !cookies.is_empty() {
                let header = match request.headers_mut().remove(COOKIE) {
                    Some(value) => HeaderValue::try_from(cookies + "; " + value.to_str()?)?,
                    None => HeaderValue::try_from(cookies)?,
                };

                request.headers_mut().try_insert(COOKIE, header)?;
            }

            // Call the test service adapter to get a response future.
            Ok(service.call(request).await?.into())
        }
    }
}

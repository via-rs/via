use cookie::CookieJar;
use http::header::COOKIE;
use http::{HeaderMap, HeaderName, HeaderValue};
use hyper::service::Service;

use super::client::Client;
use super::request::TestBody;
use crate::app::{ServiceAdapter, Shared, Via};
use crate::{Error, Router};

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

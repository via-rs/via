use cookie::{Cookie, CookieJar};
use http::header::{COOKIE, SET_COOKIE};
use http::{HeaderMap, HeaderName, HeaderValue};
use hyper::service::Service;

use super::client::Client;
use super::request::TestBody;
use crate::app::{ServiceAdapter, Via};
use crate::{Error, Response, Router, Shared};

pub struct TestService<App> {
    service: ServiceAdapter<App>,
    headers: HeaderMap,
    cookies: CookieJar,
}

/// Create a test client with the provided `router` for `app`.
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
    /// Include the provided key-value pair in the headers of each request made
    /// with this client.
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

    /// Returns reference to the shared application associated with this
    /// client.
    pub fn app(&self) -> &Shared<App> {
        self.service.app()
    }

    /// Returns a mutable reference to the cookies associated with this client.
    pub fn cookies_mut(&mut self) -> &mut CookieJar {
        &mut self.cookies
    }
}

impl<App> Client<App> for TestService<App> {
    fn send(
        &mut self,
        mut request: http::Request<TestBody>,
    ) -> impl Future<Output = crate::Result> {
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
            let response = Response::from(self.service.call(request).await?);

            // Add the cookies in the "set-cookie" headers to the client cookies.
            for value in response.headers().get_all(SET_COOKIE) {
                // Fail early if the header value is not valid UTF-8.
                let input = value.to_str()?.to_owned();

                // Fail late if the cookie could not be parsed.
                if let Ok(cookie) = Cookie::parse(input) {
                    self.cookies.add_original(cookie);
                }
            }

            Ok(response)
        }
    }
}

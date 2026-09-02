use bytes::{Buf, Bytes};
use http::{StatusCode, Uri};
use http_body_util::BodyExt;
use serde::Deserialize;
use serde::de::DeserializeOwned;

use super::request::TestBody;
use crate::{Error, Request, Response};

/// An HTTP client used to dispatch requests during tests.
pub trait Client<App>: Sized {
    /// Dispatches an HTTP request and returns the resulting response.
    fn send(&mut self, request: http::Request<TestBody>) -> impl Future<Output = crate::Result>;

    /// Sends a `CONNECT` request to `uri`.
    fn connect<T>(&mut self, uri: T) -> impl Future<Output = crate::Result>
    where
        T: TryInto<Uri>,
        <T as TryInto<Uri>>::Error: Into<http::Error>,
    {
        Request::<App>::connect(uri).send(self)
    }

    /// Sends a `DELETE` request to `uri`.
    fn delete<T>(&mut self, uri: T) -> impl Future<Output = crate::Result>
    where
        T: TryInto<Uri>,
        <T as TryInto<Uri>>::Error: Into<http::Error>,
    {
        Request::<App>::delete(uri).send(self)
    }

    /// Sends a `GET` request to `uri`.
    fn get<T>(&mut self, uri: T) -> impl Future<Output = crate::Result>
    where
        T: TryInto<Uri>,
        <T as TryInto<Uri>>::Error: Into<http::Error>,
    {
        Request::<App>::get(uri).send(self)
    }

    /// Sends a `HEAD` request to `uri`.
    fn head<T>(&mut self, uri: T) -> impl Future<Output = crate::Result>
    where
        T: TryInto<Uri>,
        <T as TryInto<Uri>>::Error: Into<http::Error>,
    {
        Request::<App>::head(uri).send(self)
    }

    /// Sends an `OPTIONS` request to `uri`.
    fn options<T>(&mut self, uri: T) -> impl Future<Output = crate::Result>
    where
        T: TryInto<Uri>,
        <T as TryInto<Uri>>::Error: Into<http::Error>,
    {
        Request::<App>::options(uri).send(self)
    }

    /// Sends a `TRACE` request to `uri`.
    fn trace<T>(&mut self, uri: T) -> impl Future<Output = crate::Result>
    where
        T: TryInto<Uri>,
        <T as TryInto<Uri>>::Error: Into<http::Error>,
    {
        Request::<App>::trace(uri).send(self)
    }
}

impl Response {
    pub async fn bytes(self) -> Result<Bytes, Error> {
        let payload = match self.into_body().collect().await {
            Ok(payload) => payload,
            Err(source) => {
                return Err(Error::from_source(source));
            }
        };

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

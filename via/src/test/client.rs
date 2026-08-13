use bytes::{Buf, Bytes};
use http::{StatusCode, Uri};
use http_body_util::BodyExt;
use serde::Deserialize;
use serde::de::DeserializeOwned;

use super::request::TestBody;
use crate::{Error, Request, Response};

pub trait Client<App>: Sized {
    fn send(&mut self, request: http::Request<TestBody>) -> impl Future<Output = crate::Result>;

    fn connect<T>(&mut self, uri: T) -> impl Future<Output = crate::Result>
    where
        T: TryInto<Uri>,
        <T as TryInto<Uri>>::Error: Into<http::Error>,
    {
        Request::<App>::connect(uri).send(self)
    }

    fn delete<T>(&mut self, uri: T) -> impl Future<Output = crate::Result>
    where
        T: TryInto<Uri>,
        <T as TryInto<Uri>>::Error: Into<http::Error>,
    {
        Request::<App>::delete(uri).send(self)
    }

    fn get<T>(&mut self, uri: T) -> impl Future<Output = crate::Result>
    where
        T: TryInto<Uri>,
        <T as TryInto<Uri>>::Error: Into<http::Error>,
    {
        Request::<App>::get(uri).send(self)
    }

    fn head<T>(&mut self, uri: T) -> impl Future<Output = crate::Result>
    where
        T: TryInto<Uri>,
        <T as TryInto<Uri>>::Error: Into<http::Error>,
    {
        Request::<App>::head(uri).send(self)
    }

    fn options<T>(&mut self, uri: T) -> impl Future<Output = crate::Result>
    where
        T: TryInto<Uri>,
        <T as TryInto<Uri>>::Error: Into<http::Error>,
    {
        Request::<App>::options(uri).send(self)
    }

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

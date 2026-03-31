use std::borrow::Cow;
use std::str::FromStr;

use super::query::{QueryParamRange, QueryParser};
use crate::error::BoxError;
use crate::util::UriEncoding;
use crate::{Error, raise};

pub struct PathParam<'a, 'b> {
    encoding: UriEncoding,
    source: &'a str,
    param: Option<&'a via_router::PathParam>,
    name: &'b str,
}

#[derive(Clone, Copy)]
pub struct PathParams<'a> {
    path: &'a str,
    spans: &'a [via_router::PathParam],
}

pub struct QueryParam<'a, 'b> {
    encoding: UriEncoding,
    source: Option<&'a str>,
    range: Option<QueryParamRange>,
    name: &'b str,
}

pub struct QueryParams<'a> {
    query: Option<&'a str>,
    spans: Vec<(Cow<'a, str>, Option<QueryParamRange>)>,
}

pub(crate) fn get<'a>(
    spans: &'a [via_router::PathParam],
    name: &str,
) -> Option<&'a via_router::PathParam> {
    spans.iter().find(|param| name == param.ident())
}

fn query_pos_for_key(
    predicate: &str,
    key: &str,
    value: &Option<[Option<usize>; 2]>,
) -> Option<[Option<usize>; 2]> {
    if key == predicate {
        value.as_ref().copied()
    } else {
        None
    }
}

impl<'a> PathParams<'a> {
    pub fn get<'b>(&self, name: &'b str) -> PathParam<'a, 'b> {
        PathParam::new(self.path, get(self.spans, name), name)
    }
}

impl<'a> PathParams<'a> {
    pub(crate) fn new(path: &'a str, spans: &'a [via_router::PathParam]) -> Self {
        Self { path, spans }
    }
}

impl<'a> QueryParams<'a> {
    pub(crate) fn new(query: Option<&'a str>) -> Self {
        let spans = query
            .map(|input| QueryParser::new(input).collect())
            .unwrap_or_default();

        Self { query, spans }
    }

    pub fn all<'b>(&self, name: &'b str) -> impl Iterator<Item = QueryParam<'a, 'b>> {
        self.spans.iter().filter_map(move |(key, value)| {
            let value = value.as_ref();

            if key.as_ref() == name {
                Some(QueryParam::new(self.query, value.copied(), name))
            } else {
                None
            }
        })
    }

    pub fn contains(&self, name: &str) -> bool {
        self.spans.iter().any(|(key, _)| key.as_ref() == name)
    }

    pub fn first<'b>(&self, name: &'b str) -> QueryParam<'a, 'b> {
        let range = self
            .spans
            .iter()
            .find_map(|(key, value)| query_pos_for_key(name, key, value));

        QueryParam::new(self.query, range, name)
    }

    pub fn last<'b>(&self, name: &'b str) -> QueryParam<'a, 'b> {
        let range = self
            .spans
            .iter()
            .rev()
            .find_map(|(key, value)| query_pos_for_key(name, key, value));

        QueryParam::new(self.query, range, name)
    }
}

impl<'a, 'b> PathParam<'a, 'b> {
    #[inline]
    pub(crate) fn new(
        source: &'a str,
        param: Option<&'a via_router::PathParam>,
        name: &'b str,
    ) -> Self {
        Self {
            encoding: UriEncoding::Unencoded,
            source,
            param,
            name,
        }
    }

    /// Returns a new `Param` that will percent-decode the parameter value with
    /// when the parameter is converted to a result.
    ///
    #[inline]
    pub fn percent_decode(self) -> Self {
        Self {
            encoding: UriEncoding::Percent,
            ..self
        }
    }

    /// Calls [`str::parse`] on the parameter value if it exists and returns the
    /// result. If the param is encoded, it will be decoded before it is parsed.
    ///
    pub fn parse<U>(self) -> Result<U, Error>
    where
        U: FromStr,
        BoxError: From<U::Err>,
    {
        self.ok_or_bad_request()?
            .as_ref()
            .parse()
            .or_else(|error| raise!(400, boxed = BoxError::from(error)))
    }

    pub fn ok(self) -> Result<Option<Cow<'a, str>>, Error> {
        self.param
            .and_then(|param| param.slice(self.source))
            .map(|value| self.encoding.decode_as(self.name, value))
            .transpose()
    }

    /// Returns a result with the parameter value if it exists. If the param is
    /// encoded, it will be decoded before it is returned.
    ///
    /// # Errors
    ///
    /// If the parameter does not exist or could not be decoded with the
    /// implementation of `T::decode`, an error is returned with a 400 Bad
    /// Request status code.
    ///
    pub fn ok_or_bad_request(self) -> Result<Cow<'a, str>, Error> {
        self.param
            .and_then(|param| param.slice(self.source))
            .ok_or_else(|| Error::require_path_param(self.name))
            .and_then(|value| self.encoding.decode_as(self.name, value))
    }
}

impl<'a, 'b> QueryParam<'a, 'b> {
    #[inline]
    pub(crate) fn new(
        source: Option<&'a str>,
        range: Option<[Option<usize>; 2]>,
        name: &'b str,
    ) -> Self {
        Self {
            encoding: UriEncoding::Unencoded,
            source,
            range,
            name,
        }
    }

    /// Returns a new `Param` that will percent-decode the parameter value with
    /// when the parameter is converted to a result.
    ///
    #[inline]
    pub fn percent_decode(self) -> Self {
        Self {
            encoding: UriEncoding::Percent,
            ..self
        }
    }

    /// Calls [`str::parse`] on the parameter value if it exists and returns the
    /// result. If the param is encoded, it will be decoded before it is parsed.
    ///
    pub fn parse<U>(self) -> Result<U, Error>
    where
        U: FromStr,
        BoxError: From<U::Err>,
    {
        self.ok_or_bad_request()?
            .as_ref()
            .parse()
            .or_else(|error| raise!(400, boxed = BoxError::from(error)))
    }

    pub fn ok(self) -> Result<Option<Cow<'a, str>>, Error> {
        self.slice()
            .map(|value| self.encoding.decode_as(self.name, value))
            .transpose()
    }

    /// Returns a result with the parameter value if it exists. If the param is
    /// encoded, it will be decoded before it is returned.
    ///
    /// # Errors
    ///
    /// If the parameter does not exist or could not be decoded with the
    /// implementation of `T::decode`, an error is returned with a 400 Bad
    /// Request status code.
    ///
    pub fn ok_or_bad_request(self) -> Result<Cow<'a, str>, Error> {
        self.slice()
            .ok_or_else(|| Error::require_query_param(self.name))
            .and_then(|value| self.encoding.decode_as(self.name, value))
    }
}

impl<'a, 'b> QueryParam<'a, 'b> {
    /// Returns a new `Param` that will percent-decode the parameter value with
    /// when the parameter is converted to a result.
    ///
    #[inline]
    fn slice(&self) -> Option<&'a str> {
        self.source
            .zip(self.range)
            .and_then(|(source, span)| match span {
                [Some(from), Some(to)] if from == to => None,
                [Some(from), Some(to)] => source.get(from..to),
                [Some(from), None] => source.get(from..),
                [None, _] => None,
            })
    }
}

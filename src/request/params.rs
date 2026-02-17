use http::StatusCode;
use std::borrow::Cow;
use std::str::FromStr;
use via_router::Ident;

use super::query::QueryParser;
use crate::util::UriEncoding;
use crate::{Error, raise};

pub(super) type ParamRange = (usize, Option<usize>);
pub(super) type PathParamEntry = (Ident, ParamRange);
pub(super) type QueryParamEntry<'a> = (Cow<'a, str>, Option<ParamRange>);

pub struct Param<'a, 'b> {
    encoding: UriEncoding,
    source: Option<&'a str>,
    name: &'b str,
    at: Option<(usize, Option<usize>)>,
}

pub struct PathParams<'a> {
    path: &'a str,
    at: &'a [PathParamEntry],
}

pub struct QueryParams<'a> {
    query: Option<&'a str>,
    at: Vec<QueryParamEntry<'a>>,
}

fn query_pos_for_key(predicate: &str, key: &str, value: &Option<ParamRange>) -> Option<ParamRange> {
    if key == predicate { *value } else { None }
}

impl<'a> PathParams<'a> {
    pub(crate) fn new(path: &'a str, at: &'a [PathParamEntry]) -> Self {
        Self { path, at }
    }

    pub fn get<'b>(&self, name: &'b str) -> Param<'a, 'b> {
        let at = self.at.iter().find_map(|(key, value)| {
            if key.as_ref() == name {
                Some(*value)
            } else {
                None
            }
        });

        Param::new(Some(self.path), name, at)
    }
}

impl<'a> QueryParams<'a> {
    pub(crate) fn new(query: Option<&'a str>) -> Self {
        let at = query
            .map(|input| QueryParser::new(input).collect())
            .unwrap_or_default();

        Self { query, at }
    }

    pub fn all<'b>(&self, name: &'b str) -> impl Iterator<Item = Param<'a, 'b>> {
        self.at.iter().filter_map(move |(key, value)| {
            if key.as_ref() == name {
                Some(Param::new(self.query, name, *value))
            } else {
                None
            }
        })
    }

    pub fn contains(&self, name: &str) -> bool {
        self.at.iter().any(|(key, _)| key.as_ref() == name)
    }

    pub fn first<'b>(&self, name: &'b str) -> Param<'a, 'b> {
        let at = self
            .at
            .iter()
            .find_map(|(key, value)| query_pos_for_key(name, key, value));

        Param::new(self.query, name, at)
    }

    pub fn last<'b>(&self, name: &'b str) -> Param<'a, 'b> {
        let at = self
            .at
            .iter()
            .rev()
            .find_map(|(key, value)| query_pos_for_key(name, key, value));

        Param::new(self.query, name, at)
    }
}

impl<'a, 'b> Param<'a, 'b> {
    fn new(source: Option<&'a str>, name: &'b str, at: Option<ParamRange>) -> Self {
        Self {
            encoding: UriEncoding::Unencoded,
            source,
            name,
            at: at.or(Some((0, Some(0)))),
        }
    }

    /// Returns a new `Param` that will percent-decode the parameter value with
    /// when the parameter is converted to a result.
    ///
    #[inline]
    pub fn decode(self) -> Self {
        Self {
            encoding: UriEncoding::Percent,
            ..self
        }
    }

    pub fn optional(self) -> Result<Option<Cow<'a, str>>, Error> {
        let Some(value) = self.source.and_then(|source| match self.at? {
            (from, Some(to)) if from == to => None,
            (from, Some(to)) => source.get(from..to),
            (from, None) => source.get(from..),
        }) else {
            return Ok(None);
        };

        self.encoding.decode(value).map(Some)
    }

    /// Calls [`str::parse`] on the parameter value if it exists and returns the
    /// result. If the param is encoded, it will be decoded before it is parsed.
    ///
    #[inline]
    pub fn parse<U>(self) -> Result<U, Error>
    where
        U: FromStr,
        U::Err: std::error::Error + Send + Sync + 'static,
    {
        self.into_result()?
            .parse()
            .or_else(|error| raise!(400, error))
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
    #[inline]
    pub fn into_result(self) -> Result<Cow<'a, str>, Error> {
        let Self { name, .. } = self;

        self.optional().and_then(|option| {
            option.ok_or_else(|| {
                Error::with_status(
                    StatusCode::BAD_REQUEST,
                    format!("missing required parameter \"{}\".", name),
                )
            })
        })
    }
}

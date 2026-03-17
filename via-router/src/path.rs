use std::fmt::{self, Debug, Formatter};
use std::ops::Deref;
use std::sync::Arc;

#[derive(Clone)]
pub struct Ident(Arc<str>);

#[derive(Debug)]
pub enum Pattern {
    Root,
    Static(Ident),
    Dynamic(Dynamic),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Dynamic {
    splat: bool,
    ident: Ident,
}

#[derive(Debug)]
pub struct PathParam {
    pattern: Dynamic,
    range: Option<[usize; 2]>,
}

#[derive(Debug)]
pub struct Segment<'a> {
    value: &'a str,
    range: [usize; 2],
}

#[derive(Clone)]
pub struct Split<'a> {
    path: &'a str,
    offset: usize,
}

/// Returns an iterator that yields a `Pattern` for each segment in the uri path.
///
pub(crate) fn patterns(path: &str) -> impl Iterator<Item = Pattern> + '_ {
    Split::new(path).map(|segment| {
        let value = segment.value();

        match value.chars().next() {
            // Path segments that start with a colon are considered a Dynamic
            // pattern. The remaining characters in the segment are considered
            // the name or identifier associated with the parameter.
            Some(':') => match value.get(1..) {
                None | Some("") => panic!("dynamic parameters must be named."),
                Some(name) => Pattern::Dynamic(Dynamic::new(name)),
            },

            // Path segments that start with an asterisk are considered CatchAll
            // pattern. The remaining characters in the segment are considered
            // the name or identifier associated with the parameter.
            Some('*') => match value.get(1..) {
                None | Some("") => panic!("splat parameters must be named."),
                Some(name) => Pattern::Dynamic(Dynamic::splat(name)),
            },

            // The segment does not start with a reserved character. We will
            // consider it a static pattern that can be matched by value.
            _ => Pattern::Static(Ident::new(value)),
        }
    })
}

impl Dynamic {
    fn new(name: &str) -> Self {
        Self {
            ident: Ident::new(name),
            splat: false,
        }
    }

    fn splat(name: &str) -> Self {
        Self {
            ident: Ident::new(name),
            splat: true,
        }
    }
}

impl Dynamic {
    pub fn ident(&self) -> &Ident {
        &self.ident
    }

    pub fn is_splat(&self) -> bool {
        self.splat
    }
}

impl Ident {
    fn new(name: &str) -> Self {
        Self(name.to_owned().into())
    }
}

impl AsRef<str> for Ident {
    fn as_ref(&self) -> &str {
        self
    }
}

impl Debug for Ident {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        Debug::fmt(self.as_ref(), f)
    }
}

impl Deref for Ident {
    type Target = str;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl PartialEq<Ident> for Ident {
    fn eq(&self, other: &Ident) -> bool {
        *self.0 == *other.0
    }
}

impl PartialEq<str> for Ident {
    fn eq(&self, rhs: &str) -> bool {
        *self.0 == *rhs
    }
}

impl PathParam {
    pub(super) fn new(pattern: Dynamic, range: Option<[usize; 2]>) -> Self {
        Self { pattern, range }
    }

    pub(super) fn pattern(&self) -> &Dynamic {
        &self.pattern
    }

    #[cfg(test)]
    pub(super) fn range(&self) -> Option<[usize; 2]> {
        self.range
    }
}

impl PathParam {
    pub fn ident(&self) -> &str {
        self.pattern().ident()
    }

    pub fn is_splat(&self) -> bool {
        self.pattern().is_splat()
    }

    pub fn slice<'a>(&self, path: &'a str) -> Option<&'a str> {
        let range = self.range?;

        if self.is_splat() {
            path.get(range[0]..)
        } else {
            path.get(range[0]..range[1])
        }
    }
}

impl Pattern {
    #[inline(always)]
    pub fn matches(&self, predicate: Option<&str>) -> bool {
        match self {
            Self::Root => true,
            Self::Static(lhs) => predicate.is_some_and(|rhs| lhs == rhs),
            Self::Dynamic(pattern) => predicate.is_some() || pattern.is_splat(),
        }
    }
}

impl<'a> Segment<'a> {
    pub fn range(&self) -> &[usize; 2] {
        &self.range
    }

    pub fn value(&self) -> &'a str {
        self.value
    }
}

impl<'a> Split<'a> {
    #[inline]
    pub fn new(path: &'a str) -> Self {
        Self {
            path,
            offset: if path.starts_with('/') { 1 } else { 0 },
        }
    }
}

impl<'a> Iterator for Split<'a> {
    type Item = Segment<'a>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let start = self.offset;

        if start == self.path.len() {
            return None;
        }

        let rest = self.path.get(start..)?;
        let len = rest
            .bytes()
            .enumerate()
            .find_map(|(index, byte)| (byte == b'/').then_some(index))
            .unwrap_or(rest.len());

        self.offset += len + 1;

        Some(Segment {
            value: rest.split_at_checked(len)?.0,
            range: [start, start + len],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::Split;

    const PATHS: [&str; 16] = [
        "/home/about",
        "/products/item/123",
        "/blog/posts/2024/june",
        "/user/profile/settings",
        "/services/contact",
        "/search/results?q=books",
        "/news/latest",
        "/portfolio/designs",
        "/faq",
        "/support/tickets",
        "//home//about",
        "/products//item/123",
        "/blog/posts/2024//june",
        "/user//profile/settings",
        "/services/contact//us",
        "/",
    ];

    fn get_expected_results() -> [Vec<[usize; 2]>; 16] {
        [
            vec![[1, 5], [6, 11]],
            vec![[1, 9], [10, 14], [15, 18]],
            vec![[1, 5], [6, 11], [12, 16], [17, 21]],
            vec![[1, 5], [6, 13], [14, 22]],
            vec![[1, 9], [10, 17]],
            vec![[1, 7], [8, 23]],
            vec![[1, 5], [6, 12]],
            vec![[1, 10], [11, 18]],
            vec![[1, 4]],
            vec![[1, 8], [9, 16]],
            vec![[1, 1], [2, 6], [7, 7], [8, 13]],
            vec![[1, 9], [10, 10], [11, 15], [16, 19]],
            vec![[1, 5], [6, 11], [12, 16], [17, 17], [18, 22]],
            vec![[1, 5], [6, 6], [7, 14], [15, 23]],
            vec![[1, 9], [10, 17], [18, 18], [19, 21]],
            vec![],
        ]
    }

    #[test]
    fn test_split_into() {
        let expected_results = get_expected_results();

        for (i, path) in PATHS.iter().enumerate() {
            let segments = Split::new(path).collect::<Vec<_>>();

            assert_eq!(
                segments.len(),
                expected_results[i].len(),
                "Split produced more or less segments than expected for {} {:#?}",
                path,
                segments,
            );

            for (j, segment) in Split::new(path).enumerate() {
                let [start, end] = expected_results[i][j];
                let expect = (&path[start..end], [start, end]);

                assert_eq!(
                    (segment.value(), *segment.range()),
                    expect,
                    "{} ({}, {:?})",
                    path,
                    j,
                    segment
                );
            }
        }
    }
}

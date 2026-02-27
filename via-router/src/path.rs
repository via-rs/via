use std::fmt::{self, Debug, Formatter};
use std::ops::Deref;

pub type Param = (usize, Option<usize>);

#[derive(Debug)]
pub enum Pattern {
    Root,
    Static(Ident),
    Dynamic(Ident),
    Splat(Ident),
}

#[derive(Clone)]
pub struct Ident(Box<str>);

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
                None | Some("") => panic!("Dynamic parameters must be named. Found ':'."),
                Some(name) => Pattern::Dynamic(name.to_owned().into()),
            },

            // Path segments that start with an asterisk are considered CatchAll
            // pattern. The remaining characters in the segment are considered
            // the name or identifier associated with the parameter.
            Some('*') => match value.get(1..) {
                None | Some("") => panic!("Wildcard parameters must be named. Found '*'."),
                Some(name) => Pattern::Splat(name.to_owned().into()),
            },

            // The segment does not start with a reserved character. We will
            // consider it a static pattern that can be matched by value.
            _ => Pattern::Static(value.to_owned().into()),
        }
    })
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

impl From<String> for Ident {
    fn from(value: String) -> Self {
        Self(value.into_boxed_str())
    }
}

impl PartialEq<str> for Ident {
    fn eq(&self, rhs: &str) -> bool {
        *self.0 == *rhs
    }
}

impl<'a> Segment<'a> {
    pub fn range_from(&self) -> (usize, Option<usize>) {
        (self.range[0], None)
    }

    pub fn range(&self) -> (usize, Option<usize>) {
        (self.range[0], Some(self.range[1]))
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
        let offset = &mut self.offset;
        let start = *offset;
        let path = self.path;

        if let Some(len) = path
            .get(start..)?
            .bytes()
            .enumerate()
            .find_map(|(index, byte)| (byte == b'/').then_some(index))
        {
            let end = start + len;
            *offset = end + 1;
            Some(Segment {
                value: &path[start..end],
                range: [start, end],
            })
        } else {
            let end = path.len();
            *offset = end;

            // Only yield if there's something left between offset and path.len().
            // Prevents slicing past the end on trailing slashes like "/via/".
            (end > start).then_some(Segment {
                value: &path[start..end],
                range: [start, end],
            })
        }
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

    fn get_expected_results() -> [Vec<(usize, Option<usize>)>; 16] {
        [
            vec![(1, Some(5)), (6, Some(11))],
            vec![(1, Some(9)), (10, Some(14)), (15, Some(18))],
            vec![(1, Some(5)), (6, Some(11)), (12, Some(16)), (17, Some(21))],
            vec![(1, Some(5)), (6, Some(13)), (14, Some(22))],
            vec![(1, Some(9)), (10, Some(17))],
            vec![(1, Some(7)), (8, Some(23))],
            vec![(1, Some(5)), (6, Some(12))],
            vec![(1, Some(10)), (11, Some(18))],
            vec![(1, Some(4))],
            vec![(1, Some(8)), (9, Some(16))],
            vec![(1, Some(1)), (2, Some(6)), (7, Some(7)), (8, Some(13))],
            vec![(1, Some(9)), (10, Some(10)), (11, Some(15)), (16, Some(19))],
            vec![
                (1, Some(5)),
                (6, Some(11)),
                (12, Some(16)),
                (17, Some(17)),
                (18, Some(22)),
            ],
            vec![(1, Some(5)), (6, Some(6)), (7, Some(14)), (15, Some(23))],
            vec![(1, Some(9)), (10, Some(17)), (18, Some(18)), (19, Some(21))],
            vec![],
        ]
    }

    #[test]
    fn test_split_into() {
        let expected_results = get_expected_results();

        for (i, path) in PATHS.iter().enumerate() {
            assert_eq!(
                Split::new(path).count(),
                expected_results[i].len(),
                "Split produced more or less segments than expected for {}",
                path
            );

            for (j, segment) in Split::new(path).enumerate() {
                let (start, end) = expected_results[i][j];
                let expect = (&path[start..end.unwrap()], (start, end));

                assert_eq!(
                    (segment.value(), segment.range()),
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

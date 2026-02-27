use smallvec::{SmallVec, smallvec};
use std::iter::Rev;
use std::marker::PhantomData;
use std::rc::Rc;
use std::slice;

use crate::path::{self, Ident, Param, Pattern, Segment, Split};

#[derive(Debug)]
pub struct Router<T> {
    tree: Node<T>,
}

/// An iterator over the middleware for a matched route.
///
pub struct Route<'a, T> {
    iter: MatchCond<slice::Iter<'a, MatchCond<T>>>,
}

pub struct RouteMut<'a, T> {
    node: &'a mut Node<T>,
    _rc: PhantomData<Rc<()>>,
}

pub struct Traverse<'a, 'b, T> {
    stack: SmallVec<[Frame<'a, 'b, T>; 1]>,
    split: Split<'b>,
}

#[derive(Debug)]
enum MatchCond<T> {
    Partial(T),
    Exact(T),
}

/// A group of nodes that match the path segment at `self.range`.
///
struct Frame<'a, 'b, T> {
    results: SmallVec<[&'a Node<T>; 1]>,
    split: Option<Split<'b>>,
    to: Option<Segment<'b>>,
}

#[derive(Debug)]
struct Node<T> {
    children: Vec<Node<T>>,
    pattern: Pattern,
    route: Vec<MatchCond<T>>,
}

impl<'a, 'b, T> Frame<'a, 'b, T> {
    fn new(node: &'a Node<T>, path: Option<Split<'b>>, segment: Option<Segment<'b>>) -> Self {
        let predicate = segment.as_ref().map(Segment::value);
        let results = node
            .children()
            .filter(|edge| match (&edge.pattern, predicate) {
                (Pattern::Static(lhs), Some(rhs)) => lhs == rhs,
                (pattern, None) => matches!(pattern, Pattern::Splat(_)),
                (_, option) => option.is_some(),
            });

        Self {
            results: results.collect(),
            split: path,
            to: segment,
        }
    }

    #[inline]
    fn segment(&self) -> Option<&Segment<'b>> {
        self.to.as_ref()
    }
}

impl<T> MatchCond<T> {
    #[inline]
    fn new(exact: bool, value: T) -> Self {
        if exact {
            Self::Exact(value)
        } else {
            Self::Partial(value)
        }
    }
}

impl<T> AsRef<T> for MatchCond<T> {
    #[inline]
    fn as_ref(&self) -> &T {
        match self {
            Self::Exact(value) | Self::Partial(value) => value,
        }
    }
}

impl<'a, T, U> Iterator for MatchCond<T>
where
    T: Iterator<Item = &'a MatchCond<U>>,
    U: 'a,
{
    type Item = &'a U;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self {
                Self::Exact(iter) => return Some(iter.next()?.as_ref()),
                Self::Partial(iter) => {
                    if let MatchCond::Partial(next) = iter.next()? {
                        return Some(next);
                    }
                }
            }
        }
    }
}

impl<T> Node<T> {
    #[inline]
    fn children(&self) -> Rev<slice::Iter<'_, Self>> {
        self.children.iter().rev()
    }

    fn push(&mut self, pattern: Pattern) -> &mut Node<T> {
        let children = &mut self.children;
        let index = children.len();

        children.push(Node {
            children: Vec::new(),
            pattern,
            route: Vec::new(),
        });

        &mut children[index]
    }

    #[inline]
    fn route(&self) -> slice::Iter<'_, MatchCond<T>> {
        self.route.iter()
    }
}

impl<T> Router<T> {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn route(&mut self, path: &'static str) -> RouteMut<'_, T> {
        RouteMut {
            node: insert(&mut self.tree, path::patterns(path)),
            _rc: PhantomData,
        }
    }

    /// Match the path argument against nodes in the route tree.
    ///
    /// # Panics
    ///
    /// If a node referenced by another node does not exist in the route tree.
    /// This router is insert-only, therefore this is a very unlikely scenario.
    ///
    pub fn traverse<'b>(&self, path: &'b str) -> Traverse<'_, 'b, T> {
        let root = Frame {
            results: smallvec![&self.tree],
            split: None,
            to: None,
        };

        Traverse {
            stack: smallvec![root],
            split: Split::new(path),
        }
    }
}

impl<T> Default for Router<T> {
    fn default() -> Self {
        Self {
            tree: Node {
                children: Vec::new(),
                pattern: Pattern::Root,
                route: Vec::new(),
            },
        }
    }
}

impl<'a, T> Route<'a, T> {
    #[inline]
    fn new(iter: MatchCond<slice::Iter<'a, MatchCond<T>>>) -> Self {
        Self { iter }
    }
}

impl<'a, T> Iterator for Route<'a, T> {
    type Item = &'a T;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }
}

impl<'a, T> RouteMut<'a, T> {
    pub fn middleware(&mut self, middleware: T) {
        self.node.route.push(MatchCond::Partial(middleware));
    }

    pub fn route(&mut self, path: &'static str) -> RouteMut<'_, T> {
        RouteMut {
            node: insert(self.node, path::patterns(path)),
            _rc: PhantomData,
        }
    }

    pub fn to(self, middleware: T) -> Self {
        self.node.route.push(MatchCond::Exact(middleware));
        self
    }
}

impl<'a, 'b, T> Iterator for Traverse<'a, 'b, T> {
    type Item = (Route<'a, T>, Option<(&'a Ident, Param)>);

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let Self { stack, split } = self;

        loop {
            let frame = stack.last_mut()?;
            let Some(node) = frame.results.pop() else {
                stack.pop();
                continue;
            };

            let param = match &node.pattern {
                Pattern::Root | Pattern::Static(_) => None,
                Pattern::Dynamic(ident) => frame.segment().map(|segment| (ident, segment.range())),
                Pattern::Splat(ident) => {
                    return Some((
                        Route::new(MatchCond::Exact(node.route())),
                        frame.segment().map(|segment| (ident, segment.range_from())),
                    ));
                }
            };

            let split = frame.split.as_mut().unwrap_or(split);
            let route = Route::new(if frame.results.is_empty() {
                *frame = Frame::new(node, None, split.next());
                MatchCond::new(frame.to.is_none(), node.route())
            } else {
                let mut split = split.clone();
                let segment = split.next();
                let iter = MatchCond::new(segment.is_none(), node.route());

                stack.push(Frame::new(node, Some(split), segment));
                iter
            });

            return Some((route, param));
        }
    }
}

fn insert<T, I>(node: &mut Node<T>, mut segments: I) -> &mut Node<T>
where
    I: Iterator<Item = Pattern>,
{
    let mut parent = node;

    loop {
        // If the current node is a catch-all, we can skip the rest of the segments.
        // In the future we may want to panic if the caller tries to insert a node
        // into a catch-all node rather than silently ignoring the rest of the
        // segments.
        if let Pattern::Splat(_) = &parent.pattern {
            return parent;
        }

        // If there are no more segments, we can return the current key.
        let pattern = match segments.next() {
            Some(value) => value,
            None => return parent,
        };

        parent = if let Some(index) =
            parent
                .children
                .iter()
                .position(|node| match (&pattern, &node.pattern) {
                    (Pattern::Static(lhs), Pattern::Static(rhs))
                    | (Pattern::Dynamic(lhs), Pattern::Dynamic(rhs))
                    | (Pattern::Splat(lhs), Pattern::Splat(rhs)) => {
                        lhs.as_bytes() == rhs.as_bytes()
                    }
                    (Pattern::Root, Pattern::Root) => true,
                    _ => false,
                }) {
            &mut parent.children[index]
        } else {
            parent.push(pattern)
        };
    }
}

#[cfg(test)]
mod tests {
    use std::iter::Map;

    use super::{Route, Router};
    use crate::path::{Ident, Param};

    const PATHS: [&str; 5] = [
        "/",
        "/echo/*path",
        "/articles/:id",
        "/articles/:id/comments",
        "/*path",
    ];

    type Match<'a, N = Ident> = (
        Map<Route<'a, String>, fn(&'a String) -> &'a str>,
        Option<(&'a N, Param)>,
    );

    macro_rules! assert_param_matches {
        ($param:expr, $pat:pat) => {
            assert!(
                matches!($param, $pat),
                "\n{} => {:?}\n",
                stringify!($pat),
                $param
            )
        };
    }

    #[allow(clippy::type_complexity)]
    fn expect_match<'a>(
        resolved: Option<(Route<'a, String>, Option<(&'a Ident, Param)>)>,
    ) -> Match<'a, str> {
        if let Some((stack, param)) = resolved {
            (
                stack.map(String::as_str),
                param.map(|(name, range)| (name.as_ref(), range)),
            )
        } else {
            panic!("unexpected end of matched routes");
        }
    }

    fn assert_matches_root((mut stack, param): Match<'_, str>) {
        assert!(matches!(stack.next(), Some("/")));
        assert!(stack.next().is_none());

        assert!(param.is_none());
    }

    #[test]
    fn test_router_resolve() {
        let mut router = Router::new();

        for path in PATHS {
            router.route(path).middleware(path.to_owned());
        }

        fn assert_matches_wildcard_at_root<'a, I, F>(results: &mut I, assert_param: F)
        where
            I: Iterator<Item = (Route<'a, String>, Option<(&'a Ident, Param)>)>,
            F: FnOnce(&Option<(&'a str, (usize, Option<usize>))>),
        {
            let (mut stack, param) = expect_match(results.next());

            assert!(matches!(stack.next(), Some("/*path")));
            assert!(stack.next().is_none());

            assert_param(&param);
        }

        //
        // visit /
        //
        // -> match "/" to root
        //     -> match "not/a/path" to Wildcard("/*path")
        //
        {
            let mut results = router.traverse("/");

            assert_matches_root(expect_match(results.next()));
            assert_matches_wildcard_at_root(&mut results, |param| {
                assert!(param.is_none());
            });

            assert!(results.next().is_none());
        }

        //
        // visit /not/a/path
        //
        // -> match "/" to root
        //     -> match "not/a/path" to Wildcard("/*path")
        //
        {
            let mut results = router.traverse("/not/a/path");

            assert_matches_root(expect_match(results.next()));
            assert_matches_wildcard_at_root(&mut results, |param| {
                assert_param_matches!(param, Some(("path", (1, None))))
            });

            assert!(results.next().is_none());
        }

        //
        // visit /echo/hello/world
        //
        // -> match "/" to root
        //     -> match "echo/hello/world" to Wildcard("/*path")
        //     -> match "echo" to Static("echo")
        //         -> match "hello/world" to Wildcard("*path")
        //
        {
            let mut results = router.traverse("/echo/hello/world");

            assert_matches_root(expect_match(results.next()));

            // Intermediate match to /echo.
            {
                let (mut stack, param) = expect_match(results.next());

                assert!(stack.next().is_none());
                assert!(param.is_none());
            }

            {
                let (mut stack, param) = expect_match(results.next());

                assert!(matches!(stack.next(), Some("/echo/*path")));
                assert!(stack.next().is_none());

                assert_param_matches!(param, Some(("path", (6, None))));
            }

            assert_matches_wildcard_at_root(&mut results, |param| {
                assert_param_matches!(param, Some(("path", (1, None))))
            });

            assert!(results.next().is_none());
        }

        //
        // visit /articles/12345
        //
        // -> match "/" to root
        //     -> match "articles/12345/comments" to Wildcard("/*path")
        //     -> match "articles" to Static("articles")
        //         -> match "12345" to Dynamic(":id")
        //
        {
            let mut results = router.traverse("/articles/12345");

            assert_matches_root(expect_match(results.next()));

            // Intermediate match to articles.
            {
                let (mut stack, param) = expect_match(results.next());

                assert!(stack.next().is_none());
                assert!(param.is_none());
            }

            {
                let (mut stack, param) = expect_match(results.next());

                assert!(matches!(stack.next(), Some("/articles/:id")));
                assert!(stack.next().is_none());

                assert_param_matches!(param, Some(("id", (10, Some(15)))));
            }

            assert_matches_wildcard_at_root(&mut results, |param| {
                assert_param_matches!(param, Some(("path", (1, None))))
            });

            assert!(results.next().is_none());
        }

        //
        // visit /articles/12345/comments
        //
        // -> match "/" to root
        //     -> match "articles/12345/comments" to Wildcard("/*path")
        //     -> match "articles" to Static("articles")
        //         -> match "12345" to Dynamic(":id")
        //             -> match "comments" to Static("comments")
        //
        {
            let mut results = router.traverse("/articles/12345/comments");

            assert_matches_root(expect_match(results.next()));

            // Intermediate match to articles.
            {
                let (mut stack, param) = expect_match(results.next());

                assert!(stack.next().is_none());
                assert!(param.is_none());
            }

            {
                let (mut stack, param) = expect_match(results.next());

                assert!(matches!(stack.next(), Some("/articles/:id")));
                assert!(stack.next().is_none());

                assert_param_matches!(param, Some(("id", (10, Some(15)))));
            }

            {
                let (mut stack, param) = expect_match(results.next());

                assert!(matches!(stack.next(), Some("/articles/:id/comments")));
                assert!(stack.next().is_none());

                assert!(param.is_none());
            }

            assert_matches_wildcard_at_root(&mut results, |param| {
                assert_param_matches!(param, Some(("path", (1, None))))
            });

            assert!(results.next().is_none());
        }
    }
}

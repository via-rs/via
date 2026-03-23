use smallvec::{SmallVec, smallvec};
use std::marker::PhantomData;
use std::rc::Rc;
use std::slice;

use crate::path::{self, PathParam, Pattern, Segment, Split};

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
        Self {
            results: node.matches(segment.as_ref().map(Segment::value)),
            split: path,
            to: segment,
        }
    }

    #[inline]
    fn segment(&self) -> Option<&Segment<'b>> {
        self.to.as_ref()
    }

    #[inline]
    fn range(&self) -> Option<&[usize; 2]> {
        self.segment().map(Segment::range)
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
    fn new(pattern: Pattern) -> Self {
        Self {
            children: Vec::new(),
            pattern,
            route: Vec::new(),
        }
    }

    #[inline(always)]
    fn matches<'a>(&'a self, predicate: Option<&str>) -> SmallVec<[&'a Node<T>; 1]> {
        self.children
            .iter()
            .rev()
            .filter(|edge| edge.pattern.matches(predicate))
            .collect()
    }

    #[inline]
    fn param(&self, range: Option<&[usize; 2]>) -> Option<PathParam> {
        if let Pattern::Dynamic(pattern) = &self.pattern {
            Some(PathParam::new(pattern.clone(), range.copied()))
        } else {
            None
        }
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

impl<T: Clone> Router<T> {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            tree: Node::new(Pattern::Root),
        }
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

impl<'a, T: Clone> Route<'a, T> {
    #[inline]
    fn new(iter: MatchCond<slice::Iter<'a, MatchCond<T>>>) -> Self {
        Self { iter }
    }
}

impl<'a, T: Clone> Iterator for Route<'a, T> {
    type Item = T;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        Some(self.iter.next()?.clone())
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

impl<'a, 'b, T> Traverse<'a, 'b, T> {
    /// Returns true if an allocation was required during traversal.
    #[cfg(feature = "benches")]
    pub fn spilled(&self) -> bool {
        self.stack.spilled()
    }

    #[cold]
    fn fork(
        &mut self,
        node: &'a Node<T>,
        mut split: Split<'b>,
    ) -> MatchCond<slice::Iter<'a, MatchCond<T>>> {
        let segment = split.next();
        let iter = MatchCond::new(segment.is_none(), node.route());

        self.stack.push(Frame::new(node, Some(split), segment));
        iter
    }
}

impl<'a, 'b, T: Clone> Iterator for Traverse<'a, 'b, T> {
    type Item = (Route<'a, T>, Option<PathParam>);

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let frame = self.stack.last_mut()?;
            let Some(node) = frame.results.pop() else {
                self.stack.pop();
                continue;
            };

            let param = node.param(frame.range());
            let route = if param.as_ref().is_some_and(PathParam::is_splat) {
                Route::new(MatchCond::Exact(node.route()))
            } else {
                let split = frame.split.as_mut().unwrap_or(&mut self.split);

                if frame.results.is_empty() {
                    *frame = Frame::new(node, None, split.next());
                    Route::new(MatchCond::new(frame.to.is_none(), node.route()))
                } else {
                    let split = split.clone();
                    Route::new(self.fork(node, split))
                }
            };

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
        if let Pattern::Dynamic(pat) = &parent.pattern
            && pat.is_splat()
        {
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
                    (Pattern::Root, Pattern::Root) => true,
                    (Pattern::Static(lhs), Pattern::Static(rhs)) => lhs == rhs,
                    (Pattern::Dynamic(lhs), Pattern::Dynamic(rhs)) => lhs == rhs,
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
    use super::{Route, Router};
    use crate::path::PathParam;

    const PATHS: [&str; 5] = [
        "/",
        "/echo/*path",
        "/articles/:id",
        "/articles/:id/comments",
        "/*path",
    ];

    type Match<'a> = (Route<'a, String>, Option<PathParam>);

    macro_rules! assert_param_matches {
        ($param:expr, $pat:pat) => {{
            let param = $param.as_ref().map(|param| (param.ident(), param.range()));

            assert!(
                matches!(param, $pat),
                "\n{} => {:?}\n",
                stringify!($pat),
                $param
            )
        }};
    }

    #[allow(clippy::type_complexity)]
    fn expect_match<'a>(resolved: Option<(Route<'a, String>, Option<PathParam>)>) -> Match<'a> {
        if let Some((stack, param)) = resolved {
            (stack, param)
        } else {
            panic!("unexpected end of matched routes");
        }
    }

    fn assert_matches_root((mut stack, param): Match<'_>) {
        assert!(matches!(stack.next().as_deref(), Some("/")));
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
            I: Iterator<Item = (Route<'a, String>, Option<PathParam>)>,
            F: FnOnce(&Option<PathParam>),
        {
            let (mut stack, param) = expect_match(results.next());

            assert!(matches!(stack.next().as_deref(), Some("/*path")));
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
                assert!(param.as_ref().and_then(PathParam::range).is_none());
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
                assert_param_matches!(param, Some(("path", Some([1, _]))))
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
                assert!(param.as_ref().and_then(PathParam::range).is_none());
            }

            {
                let (mut stack, param) = expect_match(results.next());

                assert!(matches!(stack.next().as_deref(), Some("/echo/*path")));
                assert!(stack.next().is_none());

                assert_param_matches!(param, Some(("path", Some([6, _]))));
            }

            assert_matches_wildcard_at_root(&mut results, |param| {
                assert_param_matches!(param, Some(("path", Some([1, _]))))
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
                assert!(param.as_ref().and_then(PathParam::range).is_none());
            }

            {
                let (mut stack, param) = expect_match(results.next());

                assert!(matches!(stack.next().as_deref(), Some("/articles/:id")));
                assert!(stack.next().is_none());

                assert_param_matches!(param, Some(("id", Some([10, 15]))));
            }

            assert_matches_wildcard_at_root(&mut results, |param| {
                assert_param_matches!(param, Some(("path", Some([1, _]))))
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
                assert!(param.as_ref().and_then(PathParam::range).is_none());
            }

            {
                let (mut stack, param) = expect_match(results.next());

                assert!(matches!(stack.next().as_deref(), Some("/articles/:id")));
                assert!(stack.next().is_none());

                assert_param_matches!(param, Some(("id", Some([10, 15]))));
            }

            {
                let (mut stack, param) = expect_match(results.next());

                assert!(matches!(
                    stack.next().as_deref(),
                    Some("/articles/:id/comments")
                ));
                assert!(stack.next().is_none());

                assert!(param.as_ref().and_then(PathParam::range).is_none());
            }

            assert_matches_wildcard_at_root(&mut results, |param| {
                assert_param_matches!(param, Some(("path", Some([1, _]))))
            });

            assert!(results.next().is_none());
        }
    }
}

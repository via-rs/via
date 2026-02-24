use smallvec::{SmallVec, smallvec};
use std::iter::{Peekable, Rev};
use std::marker::PhantomData;
use std::rc::Rc;
use std::slice;

use crate::path::{self, Ident, Param, Pattern, Split};

/// An iterator over the middleware for a matched route.
///
pub struct Route<'a, T>(MatchCond<slice::Iter<'a, MatchCond<T>>>);

pub struct RouteMut<'a, T> {
    node: &'a mut Node<T>,
    _rc: PhantomData<Rc<()>>,
}

#[derive(Debug)]
pub struct Router<T>(Node<T>);

pub struct Traverse<'a, 'b, T> {
    stack: SmallVec<[Frame<'a, 'b, T>; 1]>,
}

enum Vertex<'a, T> {
    Depth(Binding<'a, T>),
    Search(Rev<slice::Iter<'a, Node<T>>>),
}

#[derive(Debug)]
enum MatchCond<T> {
    Partial(T),
    Exact(T),
}

/// A group of nodes that match the path segment at `self.range`.
///
struct Binding<'a, T> {
    exhausted: bool,
    results: SmallVec<[&'a Node<T>; 1]>,
    range: Option<(usize, Option<usize>)>,
}

struct Frame<'a, 'b, T> {
    vertex: Vertex<'a, T>,
    path: Peekable<Split<'b>>,
}

#[derive(Debug)]
struct Node<T> {
    children: Vec<Node<T>>,
    pattern: Pattern,
    route: Vec<MatchCond<T>>,
}

impl<'a, 'b, T> Frame<'a, 'b, T> {
    fn search(node: &'a Node<T>, path: Peekable<Split<'b>>) -> Self {
        Self {
            vertex: Vertex::Search(node.children.iter().rev()),
            path,
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
    fn matches(&self, predicate: &str) -> bool {
        if let Pattern::Static(value) = &self.pattern {
            value == predicate
        } else {
            true
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

impl<T> Router<T> {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn route(&mut self, path: &'static str) -> RouteMut<'_, T> {
        RouteMut {
            node: insert(&mut self.0, path::patterns(path)),
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
        let mut path = Split::new(path).peekable();
        let vertex = Vertex::Depth(Binding {
            exhausted: path.peek().is_none(),
            results: smallvec![&self.0],
            range: None,
        });

        Traverse {
            stack: smallvec![Frame { vertex, path }],
        }
    }
}

impl<T> Default for Router<T> {
    fn default() -> Self {
        Self(Node {
            children: Vec::new(),
            pattern: Pattern::Root,
            route: Vec::new(),
        })
    }
}

impl<'a, T> Route<'a, T> {
    fn new(exact: bool, node: &'a Node<T>) -> Self {
        if exact {
            Self(MatchCond::Exact(node.route.iter()))
        } else {
            Self(MatchCond::Partial(node.route.iter()))
        }
    }

    fn into_exact(self) -> Self {
        match self.0 {
            exact @ MatchCond::Exact(_) => Self(exact),
            MatchCond::Partial(iter) => Self(MatchCond::Exact(iter)),
        }
    }
}

impl<'a, T> Iterator for Route<'a, T> {
    type Item = &'a T;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }
}

impl<'a, 'b, T> Iterator for Traverse<'a, 'b, T> {
    type Item = (Route<'a, T>, Option<(&'a Ident, Param)>);

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let frame = self.stack.last_mut()?;

            match &mut frame.vertex {
                Vertex::Depth(binding) => {
                    let Some(node) = binding.results.pop() else {
                        self.stack.pop();
                        continue;
                    };

                    let route = Route::new(binding.exhausted, node);

                    return Some(match &node.pattern {
                        Pattern::Root | Pattern::Static(_) => {
                            let next = Frame::search(node, frame.path.clone());

                            if binding.results.is_empty() {
                                *frame = next;
                            } else {
                                self.stack.push(next);
                            }

                            (route, None)
                        }
                        Pattern::Dynamic(ident) => {
                            let param = binding.range.map(|range| (ident, range));
                            let next = Frame::search(node, frame.path.clone());

                            if binding.results.is_empty() {
                                *frame = next;
                            } else {
                                self.stack.push(next);
                            }

                            (route, param)
                        }
                        Pattern::Splat(ident) => {
                            let param = binding.range.map(|(from, _)| (ident, (from, None)));
                            (route.into_exact(), param)
                        }
                    });
                }
                Vertex::Search(iter) => {
                    frame.vertex = if let Some((predicate, [from, to])) = frame.path.next() {
                        let results = iter.by_ref().filter(|node| node.matches(predicate));

                        Vertex::Depth(Binding {
                            exhausted: frame.path.peek().is_none(),
                            results: results.collect(),
                            range: Some((from, Some(to))),
                        })
                    } else {
                        let results = iter.by_ref().filter(|node| node.pattern.is_splat());

                        Vertex::Depth(Binding {
                            exhausted: true,
                            results: results.collect(),
                            range: None,
                        })
                    };
                }
            }
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

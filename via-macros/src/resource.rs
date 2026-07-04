/// Generate traditional REST scopes for a resource module.
///
/// ```rust
/// via::resource!(app = Unicorn);
/// ```
///
/// This macro emits up to two public functions in the current module:
///
/// ```rust
/// pub fn collection() -> impl via::Middleware<Unicorn> + 'static
/// pub fn member() -> impl via::Middleware<Unicorn> + 'static
/// ```
///
/// Path routing determines where a request belongs in the application tree.
/// The middleware returned by these functions run after path routing and
/// dispatch the request to a terminal middleware that corresponds to the
/// request method (or "verb").
///
/// <div class="warning">
/// The corresponding action functions must be visible from the call-site of
/// the <code>resource!</code> macro.
/// </div>
///
/// ```text
///    Collection
/// -----------------
///   GET  -> index
///   POST -> create
///
///      Member
/// -----------------
/// GET    -> show
/// PATCH  -> update
/// DELETE -> destroy
/// ```
///
/// ## Arguments
///
/// The only required argument is: `app`.
///
/// ```rust
/// via::resource!(app = Unicorn);
/// ```
///
/// It is used to generate scopes that return middleware that is generic over
/// your specific application type.
///
/// For example, `via::resource!(app = ());` generates:
///
/// ```rust
/// impl via::Middleware<()>
/// ```
///
/// The remaining arguments are optional and can be used to modify the behavior
/// of the middleware returned by each REST scope. They all take an optional
/// value that controls the scopes or actions to which they apply.
///
/// - `exhaustive` instructs the generated middleware to error with a
///   `405 Method Not Allowed` response if the request method is not supported.
///
/// - `guard` specifies that all scopes, a specific scope, or set of scopes and
///   / or actions take a `predicate` factory function as an argument.
///
/// - `only` is used to omit specific actions or scopes in the generated
///   output. if a scope does not have actions it is not emitted.
///
/// ### Permutations
///
/// ```rust
/// // Generate `collection` and `member` scopes.
/// via::resource!(app = Unicorn);
///
/// // Only generate the `collection` scope.
/// via::resource!(app = Unicorn, only = collection);
///
/// // Only generate the `member` scope.
/// via::resource!(app = Unicorn, only = member);
///
/// // Generate the `collection` scope with a single action: `create`.
/// via::resource!(app = Unicorn, only = create);
///
/// // Omit the `create` action from the generated scopes.
/// via::resource!(app = Unicorn, only = [index, member]);
///
/// // Both scopes take a `predicate` factory function as an argument.
/// via::resource!(app = Unicorn, guard);
///
/// // Only the `collection` scope takes a `predicate` argument.
/// via::resource!(app = Unicorn, guard = collection);
///
/// // Both the `collection` and `member` scope take a `predicate` argument
/// // but it doesn't apply to the `create` action.
/// via::resource!(app = Unicorn, guard = [index, member]);
///
/// // Any request to a generated scope with unsupported method will `405`.
/// via::resource!(app = Unicorn, exhaustive);
///
/// // Request to the `collection` scope with unsupported method will `405`.
/// via::resource!(app = Unicorn, exhaustive = collection);
///
/// // Request to the `member` scope with unsupported method will `405`.
/// via::resource!(app = Unicorn, exhaustive = member);
///
/// // Generate the `collection` and `member` scope. Exclude `create` from the
/// // `collection` scope and only apply the guard predicate factory to the
/// // `index` and `destroy` actions. Requests to either scope with an
/// // unsupported method will `405`.
/// via::resource!(
///     app = Unicorn,
///     only = [index, member],
///     guard = [index, destroy],
///     exhaustive,
/// );
/// ```
#[macro_export]
macro_rules! resource {
    (app = $app:ty $(, $($tokens:tt)*)?) => {
        $crate::__resource_parse! {
            ($app)
            [all]
            [none]
            [none]
            $($($tokens)*)?
        }
    };
}

#[macro_export]
#[doc(hidden)]
macro_rules! __resource_parse {
    (($app:ty) [$($only:tt)*] [$($guard:tt)*] [$($exhaustive:tt)*]) => {
        $crate::__resource_emit!(($app) [$($only)*] [$($guard)*] [$($exhaustive)*]);
    };

    (($app:ty) [$($only:tt)*] [$($guard:tt)*] [$($exhaustive:tt)*], $($rest:tt)*) => {
        $crate::__resource_parse!(($app) [$($only)*] [$($guard)*] [$($exhaustive)*] $($rest)*);
    };

    (($app:ty) [$($only:tt)*] [$($guard:tt)*] [$($exhaustive:tt)*] only = collection $(, $($rest:tt)*)?) => {
        $crate::__resource_parse!(($app) [collection] [$($guard)*] [$($exhaustive)*] $($($rest)*)?);
    };

    (($app:ty) [$($only:tt)*] [$($guard:tt)*] [$($exhaustive:tt)*] only = member $(, $($rest:tt)*)?) => {
        $crate::__resource_parse!(($app) [member] [$($guard)*] [$($exhaustive)*] $($($rest)*)?);
    };

    (($app:ty) [$($only:tt)*] [$($guard:tt)*] [$($exhaustive:tt)*] only = [$($action:ident),* $(,)?] $(, $($rest:tt)*)?) => {
        $crate::__resource_parse!(($app) [actions $($action),*] [$($guard)*] [$($exhaustive)*] $($($rest)*)?);
    };

    (($app:ty) [$($only:tt)*] [$($guard:tt)*] [$($exhaustive:tt)*] only = $action:ident $(, $($rest:tt)*)?) => {
        $crate::__resource_parse!(($app) [actions $action] [$($guard)*] [$($exhaustive)*] $($($rest)*)?);
    };

    (($app:ty) [$($only:tt)*] [$($guard:tt)*] [$($exhaustive:tt)*] guard $(, $($rest:tt)*)?) => {
        $crate::__resource_parse!(($app) [$($only)*] [all] [$($exhaustive)*] $($($rest)*)?);
    };

    (($app:ty) [$($only:tt)*] [$($guard:tt)*] [$($exhaustive:tt)*] guard = collection $(, $($rest:tt)*)?) => {
        $crate::__resource_parse!(($app) [$($only)*] [collection] [$($exhaustive)*] $($($rest)*)?);
    };

    (($app:ty) [$($only:tt)*] [$($guard:tt)*] [$($exhaustive:tt)*] guard = member $(, $($rest:tt)*)?) => {
        $crate::__resource_parse!(($app) [$($only)*] [member] [$($exhaustive)*] $($($rest)*)?);
    };

    (($app:ty) [$($only:tt)*] [$($guard:tt)*] [$($exhaustive:tt)*] guard = [$($action:ident),* $(,)?] $(, $($rest:tt)*)?) => {
        $crate::__resource_parse!(($app) [$($only)*] [actions $($action),*] [$($exhaustive)*] $($($rest)*)?);
    };

    (($app:ty) [$($only:tt)*] [$($guard:tt)*] [$($exhaustive:tt)*] guard = $action:ident $(, $($rest:tt)*)?) => {
        $crate::__resource_parse!(($app) [$($only)*] [actions $action] [$($exhaustive)*] $($($rest)*)?);
    };

    (($app:ty) [$($only:tt)*] [$($guard:tt)*] [$($exhaustive:tt)*] exhaustive $(, $($rest:tt)*)?) => {
        $crate::__resource_parse!(($app) [$($only)*] [$($guard)*] [all] $($($rest)*)?);
    };

    (($app:ty) [$($only:tt)*] [$($guard:tt)*] [$($exhaustive:tt)*] exhaustive = collection $(, $($rest:tt)*)?) => {
        $crate::__resource_parse!(($app) [$($only)*] [$($guard)*] [collection] $($($rest)*)?);
    };

    (($app:ty) [$($only:tt)*] [$($guard:tt)*] [$($exhaustive:tt)*] exhaustive = member $(, $($rest:tt)*)?) => {
        $crate::__resource_parse!(($app) [$($only)*] [$($guard)*] [member] $($($rest)*)?);
    };
}

#[macro_export]
#[doc(hidden)]
macro_rules! __resource_emit {
    (($app:ty) [$($only:tt)*] [$($guard:tt)*] [$($exhaustive:tt)*]) => {
        $crate::__resource_collection_from_only!(($app) [$($only)*] [$($guard)*] [$($exhaustive)*]);
        $crate::__resource_member_from_only!(($app) [$($only)*] [$($guard)*] [$($exhaustive)*]);
    };
}

// Only Scan (Collection)

#[macro_export]
#[doc(hidden)]
macro_rules! __resource_collection_from_only {
    (($app:ty) [all] [$($guard:tt)*] [$($exhaustive:tt)*]) => {
        $crate::__resource_collection_from_flags!(($app) true true [$($guard)*] [$($exhaustive)*]);
    };

    (($app:ty) [collection] [$($guard:tt)*] [$($exhaustive:tt)*]) => {
        $crate::__resource_collection_from_flags!(($app) true true [$($guard)*] [$($exhaustive)*]);
    };

    (($app:ty) [member] [$($guard:tt)*] [$($exhaustive:tt)*]) => {};

    (($app:ty) [actions $($actions:ident),*] [$($guard:tt)*] [$($exhaustive:tt)*]) => {
        $crate::__resource_collection_only_scan! {
            ($app)
            [$($guard)*]
            [$($exhaustive)*]
            [false]
            [false];
            $($actions),*
        }
    };
}

#[macro_export]
#[doc(hidden)]
macro_rules! __resource_collection_only_scan {
    (($app:ty) [$($guard:tt)*] [$($exhaustive:tt)*] [$index:tt] [$create:tt];) => {
        $crate::__resource_collection_from_flags!(($app) $index $create [$($guard)*] [$($exhaustive)*]);
    };

    (($app:ty) [$($guard:tt)*] [$($exhaustive:tt)*] [$index:tt] [$create:tt]; collection $(, $rest:ident)*) => {
        $crate::__resource_collection_only_scan!(($app) [$($guard)*] [$($exhaustive)*] [true] [true]; $($rest),*);
    };

    (($app:ty) [$($guard:tt)*] [$($exhaustive:tt)*] [$index:tt] [$create:tt]; member $(, $rest:ident)*) => {
        $crate::__resource_collection_only_scan!(($app) [$($guard)*] [$($exhaustive)*] [$index] [$create]; $($rest),*);
    };

    (($app:ty) [$($guard:tt)*] [$($exhaustive:tt)*] [$index:tt] [$create:tt]; index $(, $rest:ident)*) => {
        $crate::__resource_collection_only_scan!(($app) [$($guard)*] [$($exhaustive)*] [true] [$create]; $($rest),*);
    };

    (($app:ty) [$($guard:tt)*] [$($exhaustive:tt)*] [$index:tt] [$create:tt]; create $(, $rest:ident)*) => {
        $crate::__resource_collection_only_scan!(($app) [$($guard)*] [$($exhaustive)*] [$index] [true]; $($rest),*);
    };

    (($app:ty) [$($guard:tt)*] [$($exhaustive:tt)*] [$index:tt] [$create:tt]; $head:ident $(, $rest:ident)*) => {
        $crate::__resource_collection_only_scan!(($app) [$($guard)*] [$($exhaustive)*] [$index] [$create]; $($rest),*);
    };
}

// Only Scan (Member)

#[macro_export]
#[doc(hidden)]
macro_rules! __resource_member_from_only {
    (($app:ty) [all] [$($guard:tt)*] [$($exhaustive:tt)*]) => {
        $crate::__resource_member_from_flags!(($app) true true true [$($guard)*] [$($exhaustive)*]);
    };

    (($app:ty) [member] [$($guard:tt)*] [$($exhaustive:tt)*]) => {
        $crate::__resource_member_from_flags!(($app) true true true [$($guard)*] [$($exhaustive)*]);
    };

    (($app:ty) [collection] [$($guard:tt)*] [$($exhaustive:tt)*]) => {};

    (($app:ty) [actions $($actions:ident),*] [$($guard:tt)*] [$($exhaustive:tt)*]) => {
        $crate::__resource_member_only_scan! {
            ($app)
            [$($guard)*]
            [$($exhaustive)*]
            [false]
            [false]
            [false];
            $($actions),*
        }
    };
}

#[macro_export]
#[doc(hidden)]
macro_rules! __resource_member_only_scan {
    (($app:ty) [$($guard:tt)*] [$($exhaustive:tt)*] [$show:tt] [$update:tt] [$destroy:tt];) => {
        $crate::__resource_member_from_flags!(($app) $show $update $destroy [$($guard)*] [$($exhaustive)*]);
    };

    (($app:ty) [$($guard:tt)*] [$($exhaustive:tt)*] [$show:tt] [$update:tt] [$destroy:tt]; member $(, $rest:ident)*) => {
        $crate::__resource_member_only_scan!(($app) [$($guard)*] [$($exhaustive)*] [true] [true] [true]; $($rest),*);
    };

    (($app:ty) [$($guard:tt)*] [$($exhaustive:tt)*] [$show:tt] [$update:tt] [$destroy:tt]; collection $(, $rest:ident)*) => {
        $crate::__resource_member_only_scan!(($app) [$($guard)*] [$($exhaustive)*] [$show] [$update] [$destroy]; $($rest),*);
    };

    (($app:ty) [$($guard:tt)*] [$($exhaustive:tt)*] [$show:tt] [$update:tt] [$destroy:tt]; show $(, $rest:ident)*) => {
        $crate::__resource_member_only_scan!(($app) [$($guard)*] [$($exhaustive)*] [true] [$update] [$destroy]; $($rest),*);
    };

    (($app:ty) [$($guard:tt)*] [$($exhaustive:tt)*] [$show:tt] [$update:tt] [$destroy:tt]; update $(, $rest:ident)*) => {
        $crate::__resource_member_only_scan!(($app) [$($guard)*] [$($exhaustive)*] [$show] [true] [$destroy]; $($rest),*);
    };

    (($app:ty) [$($guard:tt)*] [$($exhaustive:tt)*] [$show:tt] [$update:tt] [$destroy:tt]; destroy $(, $rest:ident)*) => {
        $crate::__resource_member_only_scan!(($app) [$($guard)*] [$($exhaustive)*] [$show] [$update] [true]; $($rest),*);
    };

    (($app:ty) [$($guard:tt)*] [$($exhaustive:tt)*] [$show:tt] [$update:tt] [$destroy:tt]; $head:ident $(, $rest:ident)*) => {
        $crate::__resource_member_only_scan!(($app) [$($guard)*] [$($exhaustive)*] [$show] [$update] [$destroy]; $($rest),*);
    };
}

// With or Without a Predicate?

#[macro_export]
#[doc(hidden)]
macro_rules! __resource_collection_from_flags {
    (($app:ty) false false [$($guard:tt)*] [$($exhaustive:tt)*]) => {};

    (($app:ty) $index:tt $create:tt [none] [$($exhaustive:tt)*]) => {
        $crate::__resource_collection_unguarded!(($app) $index $create [$($exhaustive)*]);
    };

    (($app:ty) $index:tt $create:tt [member] [$($exhaustive:tt)*]) => {
        $crate::__resource_collection_unguarded!(($app) $index $create [$($exhaustive)*]);
    };

    (($app:ty) $index:tt $create:tt [all] [$($exhaustive:tt)*]) => {
        $crate::__resource_collection_guarded!(($app) $index $create [all] [$($exhaustive)*]);
    };

    (($app:ty) $index:tt $create:tt [collection] [$($exhaustive:tt)*]) => {
        $crate::__resource_collection_guarded!(($app) $index $create [collection] [$($exhaustive)*]);
    };

    (($app:ty) $index:tt $create:tt [actions $($actions:ident),*] [$($exhaustive:tt)*]) => {
        $crate::__resource_collection_guard_scan! {
            ($app)
            $index
            $create
            [actions $($actions),*]
            [$($exhaustive)*]
            [false];
            $($actions),*
        }
    };
}

#[macro_export]
#[doc(hidden)]
macro_rules! __resource_collection_guard_scan {
    (($app:ty) $index:tt $create:tt [$($guard:tt)*] [$($exhaustive:tt)*] [$guarded:tt];) => {
        $crate::__resource_collection_guard_choice!(
            ($app)
            $index
            $create
            [$($guard)*]
            [$($exhaustive)*]
            [$guarded]
        );
    };

    (($app:ty) true $create:tt [$($guard:tt)*] [$($exhaustive:tt)*] [$guarded:tt]; index $(, $rest:ident)*) => {
        $crate::__resource_collection_guard_scan!(
            ($app) true $create [$($guard)*] [$($exhaustive)*] [true]; $($rest),*
        );
    };

    (($app:ty) $index:tt true [$($guard:tt)*] [$($exhaustive:tt)*] [$guarded:tt]; create $(, $rest:ident)*) => {
        $crate::__resource_collection_guard_scan!(
            ($app) $index true [$($guard)*] [$($exhaustive)*] [true]; $($rest),*
        );
    };

    (($app:ty) $index:tt $create:tt [$($guard:tt)*] [$($exhaustive:tt)*] [$guarded:tt]; collection $(, $rest:ident)*) => {
        $crate::__resource_collection_guard_scan!(
            ($app) $index $create [$($guard)*] [$($exhaustive)*] [true]; $($rest),*
        );
    };

    (($app:ty) $index:tt $create:tt [$($guard:tt)*] [$($exhaustive:tt)*] [$guarded:tt]; $head:ident $(, $rest:ident)*) => {
        $crate::__resource_collection_guard_scan!(
            ($app) $index $create [$($guard)*] [$($exhaustive)*] [$guarded]; $($rest),*
        );
    };
}

#[macro_export]
#[doc(hidden)]
macro_rules! __resource_collection_guard_choice {
    (($app:ty) $index:tt $create:tt [$($guard:tt)*] [$($exhaustive:tt)*] [true]) => {
        $crate::__resource_collection_guarded!(
            ($app)
            $index
            $create
            [$($guard)*]
            [$($exhaustive)*]
        );
    };

    (($app:ty) $index:tt $create:tt [$($guard:tt)*] [$($exhaustive:tt)*] [false]) => {
        $crate::__resource_collection_unguarded!(
            ($app)
            $index
            $create
            [$($exhaustive)*]
        );
    };
}

#[macro_export]
#[doc(hidden)]
macro_rules! __resource_member_from_flags {
    (($app:ty) false false false [$($guard:tt)*] [$($exhaustive:tt)*]) => {};

    (($app:ty) $show:tt $update:tt $destroy:tt [none] [$($exhaustive:tt)*]) => {
        $crate::__resource_member_unguarded!(($app) $show $update $destroy [$($exhaustive)*]);
    };

    (($app:ty) $show:tt $update:tt $destroy:tt [collection] [$($exhaustive:tt)*]) => {
        $crate::__resource_member_unguarded!(($app) $show $update $destroy [$($exhaustive)*]);
    };

    (($app:ty) $show:tt $update:tt $destroy:tt [all] [$($exhaustive:tt)*]) => {
        $crate::__resource_member_guarded!(($app) $show $update $destroy [all] [$($exhaustive)*]);
    };

    (($app:ty) $show:tt $update:tt $destroy:tt [member] [$($exhaustive:tt)*]) => {
        $crate::__resource_member_guarded!(($app) $show $update $destroy [member] [$($exhaustive)*]);
    };

    (($app:ty) $show:tt $update:tt $destroy:tt [actions $($actions:ident),*] [$($exhaustive:tt)*]) => {
        $crate::__resource_member_guard_scan! {
            ($app)
            $show
            $update
            $destroy
            [actions $($actions),*]
            [$($exhaustive)*]
            [false];
            $($actions),*
        }
    };
}

#[macro_export]
#[doc(hidden)]
macro_rules! __resource_member_guard_scan {
    (($app:ty) $show:tt $update:tt $destroy:tt [$($guard:tt)*] [$($exhaustive:tt)*] [$guarded:tt];) => {
        $crate::__resource_member_guard_choice!(
            ($app)
            $show
            $update
            $destroy
            [$($guard)*]
            [$($exhaustive)*]
            [$guarded]
        );
    };

    (($app:ty) true $update:tt $destroy:tt [$($guard:tt)*] [$($exhaustive:tt)*] [$guarded:tt]; show $(, $rest:ident)*) => {
        $crate::__resource_member_guard_scan!(
            ($app) true $update $destroy [$($guard)*] [$($exhaustive)*] [true]; $($rest),*
        );
    };

    (($app:ty) $show:tt true $destroy:tt [$($guard:tt)*] [$($exhaustive:tt)*] [$guarded:tt]; update $(, $rest:ident)*) => {
        $crate::__resource_member_guard_scan!(
            ($app) $show true $destroy [$($guard)*] [$($exhaustive)*] [true]; $($rest),*
        );
    };

    (($app:ty) $show:tt $update:tt true [$($guard:tt)*] [$($exhaustive:tt)*] [$guarded:tt]; destroy $(, $rest:ident)*) => {
        $crate::__resource_member_guard_scan!(
            ($app) $show $update true [$($guard)*] [$($exhaustive)*] [true]; $($rest),*
        );
    };

    (($app:ty) $show:tt $update:tt $destroy:tt [$($guard:tt)*] [$($exhaustive:tt)*] [$guarded:tt]; member $(, $rest:ident)*) => {
        $crate::__resource_member_guard_scan!(
            ($app) $show $update $destroy [$($guard)*] [$($exhaustive)*] [true]; $($rest),*
        );
    };

    (($app:ty) $show:tt $update:tt $destroy:tt [$($guard:tt)*] [$($exhaustive:tt)*] [$guarded:tt]; $head:ident $(, $rest:ident)*) => {
        $crate::__resource_member_guard_scan!(
            ($app) $show $update $destroy [$($guard)*] [$($exhaustive)*] [$guarded]; $($rest),*
        );
    };
}

#[macro_export]
#[doc(hidden)]
macro_rules! __resource_member_guard_choice {
    (($app:ty) $show:tt $update:tt $destroy:tt [$($guard:tt)*] [$($exhaustive:tt)*] [true]) => {
        $crate::__resource_member_guarded!(
            ($app)
            $show
            $update
            $destroy
            [$($guard)*]
            [$($exhaustive)*]
        );
    };

    (($app:ty) $show:tt $update:tt $destroy:tt [$($guard:tt)*] [$($exhaustive:tt)*] [false]) => {
        $crate::__resource_member_unguarded!(
            ($app)
            $show
            $update
            $destroy
            [$($exhaustive)*]
        );
    };
}

// Scopes

#[macro_export]
#[doc(hidden)]
macro_rules! __resource_collection_unguarded {
    (($app:ty) $index:tt $create:tt [$($exhaustive:tt)*]) => {
        pub fn collection() -> impl via::Middleware<$app> + 'static {
            $crate::__resource_collection_switch!(unguarded $index $create [$($exhaustive)*])
        }
    };
}

#[macro_export]
#[doc(hidden)]
macro_rules! __resource_collection_guarded {
    (($app:ty) $index:tt $create:tt [$($guard:tt)*] [$($exhaustive:tt)*]) => {
        pub fn collection<T>(predicate: impl Fn() -> T) -> impl via::Middleware<$app> + 'static
        where
            T: via::guard::Predicate<via::Request<$app>> + Send + Sync + 'static,
            for<'a> T::Error<'a>: Into<via::Error>,
        {
            $crate::__resource_collection_switch!(guarded predicate [$($guard)*] $index $create [$($exhaustive)*])
        }
    };
}

#[macro_export]
#[doc(hidden)]
macro_rules! __resource_member_unguarded {
    (($app:ty) $show:tt $update:tt $destroy:tt [$($exhaustive:tt)*]) => {
        pub fn member() -> impl via::Middleware<$app> + 'static {
            $crate::__resource_member_switch!(unguarded $show $update $destroy [$($exhaustive)*])
        }
    };
}

#[macro_export]
#[doc(hidden)]
macro_rules! __resource_member_guarded {
    (($app:ty) $show:tt $update:tt $destroy:tt [$($guard:tt)*] [$($exhaustive:tt)*]) => {
        pub fn member<T>(predicate: impl Fn() -> T) -> impl via::Middleware<$app> + 'static
        where
            T: via::guard::Predicate<via::Request<$app>> + Send + Sync + 'static,
            for<'a> T::Error<'a>: Into<via::Error>,
        {
            $crate::__resource_member_switch!(guarded predicate [$($guard)*] $show $update $destroy [$($exhaustive)*])
        }
    };
}

// Switches

#[macro_export]
#[doc(hidden)]
macro_rules! __resource_collection_switch {
    (unguarded true true [$($exhaustive:tt)*]) => {{
        let switch = via::get(index).post(create);
        $crate::__resource_maybe_deny!(collection [$($exhaustive)*] switch)
    }};

    (unguarded true false [$($exhaustive:tt)*]) => {{
        let switch = via::get(index);
        $crate::__resource_maybe_deny!(collection [$($exhaustive)*] switch)
    }};

    (unguarded false true [$($exhaustive:tt)*]) => {{
        let switch = via::post(create);
        $crate::__resource_maybe_deny!(collection [$($exhaustive)*] switch)
    }};

    (guarded $predicate:ident [$($guard:tt)*] true true [$($exhaustive:tt)*]) => {{
        let switch = via::get($crate::__resource_guard_action!($predicate [$($guard)*] index))
            .post($crate::__resource_guard_action!($predicate [$($guard)*] create));

        $crate::__resource_maybe_deny!(collection [$($exhaustive)*] switch)
    }};

    (guarded $predicate:ident [$($guard:tt)*] true false [$($exhaustive:tt)*]) => {{
        let switch = via::get($crate::__resource_guard_action!($predicate [$($guard)*] index));
        $crate::__resource_maybe_deny!(collection [$($exhaustive)*] switch)
    }};

    (guarded $predicate:ident [$($guard:tt)*] false true [$($exhaustive:tt)*]) => {{
        let switch = via::post($crate::__resource_guard_action!($predicate [$($guard)*] create));
        $crate::__resource_maybe_deny!(collection [$($exhaustive)*] switch)
    }};
}

#[macro_export]
#[doc(hidden)]
macro_rules! __resource_member_switch {
    (unguarded true true true [$($exhaustive:tt)*]) => {{
        let switch = via::get(show).patch(update).delete(destroy);
        $crate::__resource_maybe_deny!(member [$($exhaustive)*] switch)
    }};

    (unguarded true true false [$($exhaustive:tt)*]) => {{
        let switch = via::get(show).patch(update);
        $crate::__resource_maybe_deny!(member [$($exhaustive)*] switch)
    }};

    (unguarded true false true [$($exhaustive:tt)*]) => {{
        let switch = via::get(show).delete(destroy);
        $crate::__resource_maybe_deny!(member [$($exhaustive)*] switch)
    }};

    (unguarded false true true [$($exhaustive:tt)*]) => {{
        let switch = via::patch(update).delete(destroy);
        $crate::__resource_maybe_deny!(member [$($exhaustive)*] switch)
    }};

    (unguarded true false false [$($exhaustive:tt)*]) => {{
        let switch = via::get(show);
        $crate::__resource_maybe_deny!(member [$($exhaustive)*] switch)
    }};

    (unguarded false true false [$($exhaustive:tt)*]) => {{
        let switch = via::patch(update);
        $crate::__resource_maybe_deny!(member [$($exhaustive)*] switch)
    }};

    (unguarded false false true [$($exhaustive:tt)*]) => {{
        let switch = via::delete(destroy);
        $crate::__resource_maybe_deny!(member [$($exhaustive)*] switch)
    }};

    (guarded $predicate:ident [$($guard:tt)*] true true true [$($exhaustive:tt)*]) => {{
        let switch = via::get($crate::__resource_guard_action!($predicate [$($guard)*] show))
            .patch($crate::__resource_guard_action!($predicate [$($guard)*] update))
            .delete($crate::__resource_guard_action!($predicate [$($guard)*] destroy));

        $crate::__resource_maybe_deny!(member [$($exhaustive)*] switch)
    }};

    (guarded $predicate:ident [$($guard:tt)*] true true false [$($exhaustive:tt)*]) => {{
        let switch = via::get($crate::__resource_guard_action!($predicate [$($guard)*] show))
            .patch($crate::__resource_guard_action!($predicate [$($guard)*] update));

        $crate::__resource_maybe_deny!(member [$($exhaustive)*] switch)
    }};

    (guarded $predicate:ident [$($guard:tt)*] true false true [$($exhaustive:tt)*]) => {{
        let switch = via::get($crate::__resource_guard_action!($predicate [$($guard)*] show))
            .delete($crate::__resource_guard_action!($predicate [$($guard)*] destroy));

        $crate::__resource_maybe_deny!(member [$($exhaustive)*] switch)
    }};

    (guarded $predicate:ident [$($guard:tt)*] false true true [$($exhaustive:tt)*]) => {{
        let switch = via::patch($crate::__resource_guard_action!($predicate [$($guard)*] update))
            .delete($crate::__resource_guard_action!($predicate [$($guard)*] destroy));

        $crate::__resource_maybe_deny!(member [$($exhaustive)*] switch)
    }};

    (guarded $predicate:ident [$($guard:tt)*] true false false [$($exhaustive:tt)*]) => {{
        let switch = via::get($crate::__resource_guard_action!($predicate [$($guard)*] show));
        $crate::__resource_maybe_deny!(member [$($exhaustive)*] switch)
    }};

    (guarded $predicate:ident [$($guard:tt)*] false true false [$($exhaustive:tt)*]) => {{
        let switch = via::patch($crate::__resource_guard_action!($predicate [$($guard)*] update));
        $crate::__resource_maybe_deny!(member [$($exhaustive)*] switch)
    }};

    (guarded $predicate:ident [$($guard:tt)*] false false true [$($exhaustive:tt)*]) => {{
        let switch = via::delete($crate::__resource_guard_action!($predicate [$($guard)*] destroy));
        $crate::__resource_maybe_deny!(member [$($exhaustive)*] switch)
    }};
}

// Guards

#[macro_export]
#[doc(hidden)]
macro_rules! __resource_guard_action {
    ($predicate:ident [all] $action:ident) => {
        via::guard::flat_map($predicate(), $action)
    };

    ($predicate:ident [collection] index) => {
        via::guard::flat_map($predicate(), index)
    };

    ($predicate:ident [collection] create) => {
        via::guard::flat_map($predicate(), create)
    };

    ($predicate:ident [member] show) => {
        via::guard::flat_map($predicate(), show)
    };

    ($predicate:ident [member] update) => {
        via::guard::flat_map($predicate(), update)
    };

    ($predicate:ident [member] destroy) => {
        via::guard::flat_map($predicate(), destroy)
    };

    ($predicate:ident [actions $($actions:ident),*] $action:ident) => {
        $crate::__resource_guard_action_list!($predicate $action; $($actions),*)
    };

    ($predicate:ident [$($guard:tt)*] $action:ident) => {
        $action
    };
}

#[macro_export]
#[doc(hidden)]
macro_rules! __resource_guard_action_list {
    ($predicate:ident $action:ident;) => {
        $action
    };

    ($predicate:ident index; collection $(, $rest:ident)*) => {
        via::guard::flat_map($predicate(), index)
    };

    ($predicate:ident create; collection $(, $rest:ident)*) => {
        via::guard::flat_map($predicate(), create)
    };

    ($predicate:ident show; collection $(, $rest:ident)*) => {
        $crate::__resource_guard_action_list!($predicate show; $($rest),*)
    };

    ($predicate:ident update; collection $(, $rest:ident)*) => {
        $crate::__resource_guard_action_list!($predicate update; $($rest),*)
    };

    ($predicate:ident destroy; collection $(, $rest:ident)*) => {
        $crate::__resource_guard_action_list!($predicate destroy; $($rest),*)
    };

    ($predicate:ident index; member $(, $rest:ident)*) => {
        $crate::__resource_guard_action_list!($predicate index; $($rest),*)
    };

    ($predicate:ident create; member $(, $rest:ident)*) => {
        $crate::__resource_guard_action_list!($predicate create; $($rest),*)
    };

    ($predicate:ident show; member $(, $rest:ident)*) => {
        via::guard::flat_map($predicate(), show)
    };

    ($predicate:ident update; member $(, $rest:ident)*) => {
        via::guard::flat_map($predicate(), update)
    };

    ($predicate:ident destroy; member $(, $rest:ident)*) => {
        via::guard::flat_map($predicate(), destroy)
    };

    ($predicate:ident index; index $(, $rest:ident)*) => {
        via::guard::flat_map($predicate(), index)
    };

    ($predicate:ident create; create $(, $rest:ident)*) => {
        via::guard::flat_map($predicate(), create)
    };

    ($predicate:ident show; show $(, $rest:ident)*) => {
        via::guard::flat_map($predicate(), show)
    };

    ($predicate:ident update; update $(, $rest:ident)*) => {
        via::guard::flat_map($predicate(), update)
    };

    ($predicate:ident destroy; destroy $(, $rest:ident)*) => {
        via::guard::flat_map($predicate(), destroy)
    };

    ($predicate:ident $action:ident; $head:ident $(, $rest:ident)*) => {
        $crate::__resource_guard_action_list!($predicate $action; $($rest),*)
    };
}

// Exhuastive

#[macro_export]
#[doc(hidden)]
macro_rules! __resource_maybe_deny {
    ($scope:ident [all] $switch:ident) => {
        $switch.or_deny()
    };

    (collection [collection] $switch:ident) => {
        $switch.or_deny()
    };

    (member [member] $switch:ident) => {
        $switch.or_deny()
    };

    ($scope:ident [$($exhaustive:tt)*] $switch:ident) => {
        $switch
    };
}

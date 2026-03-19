#[macro_export]
macro_rules! resource {
    ($mod:ident, $param:literal) => {
        $crate::resource!(
            #[expr]
            $mod,
            concat!("/", stringify!($mod)),
            concat!("/", stringify!($mod), "/", $param)
        )
    };

    ($mod:ident, $param:literal, $name:literal) => {
        $crate::resource!(
            #[expr]
            $mod,
            concat!("/", $name),
            concat!("/", $name, "/", $param)
        )
    };

    (#[expr] $mod:ident, $collection:expr, $member:expr) => {{
        let (collection, member) = $crate::rest!($mod);
        $crate::router::Resource::new(($collection, collection), ($member, member))
    }};
}

#[macro_export]
macro_rules! rest {
    ($mod:path) => {
        (
            $crate::rest!($mod as collection),
            $crate::rest!($mod as member),
        )
    };
    ($mod:path as collection) => {{
        use $mod::{create, index};
        $crate::post(create).get(index)
    }};
    ($mod:path as member) => {{
        use $mod::{destroy, show, update};
        $crate::delete(destroy).patch(update).get(show)
    }};
    ($mod:path as $other:ident) => {{
        compile_error!(concat!(
            "incorrect rest! modifier \"",
            stringify!($other),
            "\"",
        ));
    }};

    ($mod:ident, $param:literal) => {
        $crate::resource!(
            #[expr]
            $mod,
            concat!("/", stringify!($mod)),
            concat!("/", stringify!($mod), "/", $param)
        )
    };

    ($mod:ident, $param:literal, $name:literal) => {
        $crate::resource!(
            #[expr]
            $mod,
            concat!("/", $name),
            concat!("/", $name, "/", $param)
        )
    };

    (#[expr] $mod:ident, $collection:expr, $member:expr) => {{
        let (collection, member) = $crate::rest!($mod);
        $crate::router::Resource::new(($collection, collection), ($member, member))
    }};
}

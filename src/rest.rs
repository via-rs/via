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

    ($mod:ident, $param:literal) => {{
        assert!(
            $param.starts_with(|start| start == ':' || start == '*'),
            "parameter names must start with either : or *."
        );

        $crate::rest!(
            #[resource]
            $mod,
            concat!("/", stringify!($mod)),
            concat!("/", stringify!($mod), "/", $param)
        )
    }};

    ($mod:ident, $param:literal, $name:literal) => {{
        assert!(
            $param.starts_with(|start| start == ':' || start == '*'),
            "parameter names must start with either : or *."
        );

        $crate::rest!(
            #[resource]
            $mod,
            concat!("/", $name),
            concat!("/", $name, "/", $param)
        )
    }};

    (#[resource] $mod:ident, $collection:expr, $member:expr) => {
        $crate::router::ResourceBuilder::collection($collection, $crate::rest!($mod as collection))
            .member($member, $crate::rest!($mod as member))
    };
}

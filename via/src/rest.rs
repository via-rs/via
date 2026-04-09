#[macro_export]
macro_rules! collection {
    ($mod:path) => {{
        use $mod::{create, index};
        $crate::post(create).get(index)
    }};
}

#[macro_export]
macro_rules! member {
    ($mod:path) => {{
        use $mod::{destroy, show, update};
        $crate::delete(destroy).patch(update).get(show)
    }};
}

#[macro_export]
macro_rules! rest {
    ($mod:path, $param:literal) => {{
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

    ($mod:path, $param:literal, $name:literal) => {{
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

    (#[resource] $mod:path, $collection:expr, $member:expr) => {
        $crate::router::ResourceBuilder::collection($collection, $crate::collection!($mod))
            .member($member, $crate::member!($mod))
    };
}

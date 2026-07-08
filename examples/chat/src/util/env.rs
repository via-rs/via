use zeroize::Zeroizing;

pub fn require(name: &str) -> Zeroizing<String> {
    let Ok(value) = dotenvy::var(name) else {
        panic!("missing required env variable: {}", name);
    };

    value.into()
}

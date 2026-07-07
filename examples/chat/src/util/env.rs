use std::env::{VarError, var};

pub fn require(name: &str) -> String {
    var(name).unwrap_or_else(|error| match error {
        VarError::NotPresent => {
            panic!("missing required env variable: {}", name);
        }
        VarError::NotUnicode(_) => {
            panic!("env variable \"{}\" is not valid UTF-8", name);
        }
    })
}

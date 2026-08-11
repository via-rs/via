pub mod session;

mod id;
mod timestamp;

pub use id::Id;
pub use session::{Authenticator, Session};
pub use timestamp::Iso8601;

#[cfg(test)]
pub async fn setup_integration_test() -> via::Result<impl via::test::Client> {
    use std::path::{Path, PathBuf};

    dotenvy::from_filename({
        let path = Path::new(env!("CARGO_MANIFEST_DIR"));

        if path.ends_with("examples") {
            path.join("chat/.env")
        } else {
            path.join("examples/chat/.env")
        }
    });

    let router = via::Router::new(crate::routes);
    let (_, unicorn) = crate::Unicorn::new().await?;

    Ok(via::test::service(router, unicorn))
}

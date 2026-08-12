pub mod session;

mod id;
mod timestamp;

pub use id::Id;
pub use session::{Authenticator, Session};
pub use timestamp::Iso8601;

#[cfg(test)]
pub mod test {
    use http::header::ACCEPT;
    use std::path::{Path, PathBuf};
    use via::test::{TestService, service};

    use super::session::Authenticator;
    use crate::app::{SESSION, Unicorn};
    use crate::models::User;

    pub type Client = TestService<Unicorn>;

    pub fn login(client: &mut Client, user: User) -> via::Result<()> {
        // Keeping a clone of the signing key on the stack is fine for tests.
        // However, this is not something we would do in release builds.
        let signer = client.app().signer().clone();

        let Some(session) = client
            .app()
            .login(user)?
            .cookies()
            .signed(&signer)
            .get(SESSION)
            .map(|cookie| cookie.to_owned())
        else {
            via::deny!(401, "unauthorized");
        };

        client
            .cookies_mut()
            .signed_mut(&signer)
            .add_original(session);

        Ok(())
    }

    pub async fn setup() -> via::Result<Client> {
        dotenvy::from_filename(&resolve_env_file())?;

        let router = via::Router::new(crate::routes);
        let (_, unicorn) = Unicorn::new().await?;

        service(router, unicorn).header(ACCEPT, "application/json; charset=utf-8")
    }

    fn resolve_env_file() -> PathBuf {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"));

        if path.ends_with("examples") {
            path.join("chat/.env")
        } else {
            path.join("examples/chat/.env")
        }
    }
}

pub mod session;

mod id;
mod timestamp;

pub use id::Id;
pub use session::{Authenticator, Session};
pub use timestamp::Iso8601;

#[cfg(test)]
pub mod test {
    use diesel::Identifiable;
    use http::header::ACCEPT;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};
    use via::test::{TestService, service};

    use super::session::Authenticator;
    use crate::app::{SESSION, Unicorn};
    use crate::models::user::{NewUser, User};

    const DROWSSAP: &str = "drowssap";

    pub type Client = TestService<Unicorn>;

    /// Create a test user and authenticate a session for them in `client`.
    pub async fn login(client: &mut Client) -> via::Result<User> {
        let user = {
            // The time in milliseconds that have passed since the UNIX epoch.
            let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();

            // Perform a couple of allocations before checking out a database
            // connection. We just used `clock_gettime` with a NON-monotonic
            // clock.
            //
            // In production, it is a best practice to use `Instant::now()` as
            // it uses `CLOCK_MONOTONIC` with `clock_gettime`. The only reason
            // we use `SystemTime` in tests is to teach it's edges.

            let email = format!("test-user-{}@kontinue.boo", now);
            let username = format!("test-user-{}", now);

            // Zeroizing behavior in tests is not pedantic.
            //
            // For example, the plaintext value of the password is a `const`.
            //
            // This is preferred to giving other modules authority to hash
            // passwords. Albeit it is tempting to zeroize everything.

            let password = serde_json::from_str(&format!("\"{}\"", DROWSSAP))?;
            let confirm_password = DROWSSAP.to_owned().into();

            User::create(
                &mut client.app().database().await?,
                NewUser::new(email, username, password, confirm_password),
            )
            .await?
        };

        // Keeping a clone of the signing key on the stack is fine for tests.
        // However, this is not something we would do in release builds.
        let signer = client.app().signer().clone();

        // Authenticate the user using the implementation of `Authenticator`
        // for `Unicorn`. Then, extract the cookie from the returned response.
        let Some(session) = client
            .app()
            .login(user.clone())?
            .cookies()
            .signed(&signer)
            .get(SESSION)
            .map(|cookie| cookie.to_owned())
        else {
            via::deny!(401, "unauthorized");
        };

        // Add the session cookie to the client's signed cookie jar with the
        // same signing key that would have been used to generate the
        // "set-cookie" header.
        client
            .cookies_mut()
            .signed_mut(&signer)
            .add_original(session);

        Ok(user)
    }

    /// Destroy the test user and remove the client's session cookie.
    pub async fn logout(client: &mut Client, user: User) -> via::Result<()> {
        let affected_rows = {
            // Checkout a database connection.
            let mut connection = client.app().database().await?;

            // Destroy the user.
            User::destroy(&mut connection, *user.id()).await?
        };

        // If the number of rows affected is < 1, 404 not found.
        if let ..1 = affected_rows {
            via::deny!(404, "not found");
        };

        // Remove the session cookie.
        client.cookies_mut().remove(SESSION);

        Ok(())
    }

    /// Create a test client for `unicorn` with the routes defined in `main.rs`.
    pub async fn setup() -> via::Result<Client> {
        dotenvy::from_filename(&resolve_env_file())?;

        let router = via::Router::new(crate::routes);
        let (_, unicorn) = Unicorn::new().await?;

        service(router, unicorn).header(ACCEPT, "application/json; charset=utf-8")
    }

    /// Resolves the path to the `.env` file regardless of the working
    /// directory in which the `cargo` command is executed.
    fn resolve_env_file() -> PathBuf {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"));

        if path.ends_with("examples") {
            path.join("chat/.env")
        } else {
            path.join("examples/chat/.env")
        }
    }
}

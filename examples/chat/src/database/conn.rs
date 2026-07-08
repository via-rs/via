use bb8::{Pool, PooledConnection};
use diesel_async::AsyncPgConnection;
use diesel_async::pooled_connection::AsyncDieselConnectionManager;

use crate::util::env;

pub type Connection<'a> = PooledConnection<'a, ConnectionManager>;
pub type ConnectionPool = Pool<ConnectionManager>;
pub type ConnectionManager = AsyncDieselConnectionManager<AsyncPgConnection>;

pub async fn establish_connection() -> ConnectionPool {
    let connection_manager = ConnectionManager::new(&*env::require("DATABASE_URL"));
    let result = Pool::builder().build(connection_manager).await;

    result.unwrap_or_else(|error| {
        panic!("failed to establish database connection: error = {}", error);
    })
}

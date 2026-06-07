pub mod models;

mod schema;
mod table;

pub use schema::Database;
pub use table::{Id, Identify, Persist};

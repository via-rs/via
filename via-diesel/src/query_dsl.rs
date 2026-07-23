use diesel::backend::Backend;
use diesel::dsl::Limit;
use diesel::query_builder::QueryFragment;
use diesel::result::{DatabaseErrorKind, Error as DieselError, QueryResult};
use diesel_async::methods::{ExecuteDsl, LoadQuery};
use diesel_async::return_futures::{GetResult, LoadFuture};
use diesel_async::{AsyncConnectionCore, RunQueryDsl};
use http::StatusCode;
use std::pin::Pin;
use std::task::{Context, Poll};
use via::err;

pub trait AsyncQueryDsl<T>: Sized {
    fn execute_async<'a, 'b>(self, connection: &'a mut T) -> MapErr<T::ExecuteFuture<'a, 'b>>
    where
        T: AsyncConnectionCore + Send,
        T::Backend: Default,
        <T::Backend as Backend>::QueryBuilder: Default,
        Self: ExecuteDsl<T> + QueryFragment<T::Backend> + 'b,
    {
        #[cfg(debug_assertions)]
        debug_query(&self);

        MapErr {
            future: RunQueryDsl::execute(self, connection),
        }
    }

    fn first_async<'a, 'b, U>(
        self,
        connection: &'a mut T,
    ) -> MapErr<GetResult<'a, 'b, Limit<Self>, T, U>>
    where
        T: AsyncConnectionCore + Send,
        T::Backend: Default,
        <T::Backend as Backend>::QueryBuilder: Default,
        U: Send + 'a,
        Self: diesel::query_dsl::methods::LimitDsl,
        Limit<Self>: LoadQuery<'b, T, U> + QueryFragment<T::Backend> + Send + 'b,
    {
        AsyncQueryDsl::get_result_async(self.limit(1), connection)
    }

    fn load_async<'a, 'b, U>(self, connection: &'a mut T) -> MapErr<LoadFuture<'a, 'b, Self, T, U>>
    where
        T: AsyncConnectionCore + Send,
        T::Backend: Default,
        <T::Backend as Backend>::QueryBuilder: Default,
        U: Send,
        Self: LoadQuery<'b, T, U> + QueryFragment<T::Backend> + 'b,
    {
        #[cfg(debug_assertions)]
        debug_query(&self);

        MapErr {
            future: RunQueryDsl::load(self, connection),
        }
    }

    fn get_result_async<'a, 'b, U>(
        self,
        connection: &'a mut T,
    ) -> MapErr<GetResult<'a, 'b, Self, T, U>>
    where
        T: AsyncConnectionCore + Send,
        T::Backend: Default,
        <T::Backend as Backend>::QueryBuilder: Default,
        U: Send + 'a,
        Self: LoadQuery<'b, T, U> + QueryFragment<T::Backend> + Send + 'b,
    {
        #[cfg(debug_assertions)]
        debug_query(&self);

        MapErr {
            future: RunQueryDsl::get_result(self, connection),
        }
    }
}

/// Convert errors occuring in a future returned by a query into an [`Error`].
///
/// [`Error`]: via::Error
pub struct MapErr<F> {
    future: F,
}

/// Prints the generated SQL query to the console in debug builds.
#[cfg(debug_assertions)]
fn debug_query<T, U>(query: &T)
where
    T: QueryFragment<U>,
    U: Backend + Default,
    U::QueryBuilder: Default,
{
    println!("info(database): {}", diesel::debug_query::<U, _>(query));
}

/// Determine the appropriate status code for a database error kind.
fn status_for_database_error(kind: &DatabaseErrorKind) -> StatusCode {
    match kind {
        // A required column value is not present in the operation arguments.
        DatabaseErrorKind::NotNullViolation => StatusCode::BAD_REQUEST,

        // The operation conflicts with the current state of the system.
        DatabaseErrorKind::ExclusionViolation
        | DatabaseErrorKind::ForeignKeyViolation
        | DatabaseErrorKind::RestrictViolation
        | DatabaseErrorKind::SerializationFailure
        | DatabaseErrorKind::UniqueViolation => StatusCode::CONFLICT,

        // The result of the operation would violate semantic rules.
        DatabaseErrorKind::CheckViolation => StatusCode::UNPROCESSABLE_ENTITY,

        // The database is temporarily unavailable.
        DatabaseErrorKind::ClosedConnection => StatusCode::SERVICE_UNAVAILABLE,

        // All other errors are opaque.
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

impl<T, U> AsyncQueryDsl<T> for U where U: RunQueryDsl<T> {}

impl<F, T> Future for MapErr<F>
where
    F: Future<Output = QueryResult<T>> + Unpin,
{
    type Output = via::Result<T>;

    fn poll(mut self: Pin<&mut Self>, context: &mut Context) -> Poll<Self::Output> {
        match Pin::new(&mut self.future).poll(context) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Ok(data)) => Poll::Ready(Ok(data)),
            Poll::Ready(Err(ref error)) => {
                #[cfg(debug_assertions)]
                eprintln!("error(database): {}", error);

                // Placeholder for tracing...

                match error {
                    // No rows were returned by a query expected to return at least one row.
                    DieselError::NotFound => Poll::Ready(Err(err!(404, "not found"))),

                    // An error was returned by the database.
                    DieselError::DatabaseError(kind, info) => {
                        let status = status_for_database_error(kind);
                        let message = info.message();

                        Poll::Ready(Err(err!(status, "{}", message)))
                    }

                    // The error occured for some other reason.
                    _ => Poll::Ready(Err(err!(500, "internal server error"))),
                }
            }
        }
    }
}

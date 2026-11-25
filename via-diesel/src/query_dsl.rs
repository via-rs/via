use diesel::backend::Backend;
use diesel::dsl::Limit;
use diesel::query_builder::QueryFragment;
use diesel::query_dsl::methods::LimitDsl;
use diesel::result::{DatabaseErrorKind, Error as DieselError, QueryResult};
use diesel_async::methods::{ExecuteDsl, LoadQuery};
use diesel_async::return_futures::{GetResult, LoadFuture};
use diesel_async::{AsyncConnectionCore, RunQueryDsl};
use http::StatusCode;
use std::pin::Pin;
use std::task::{Context, Poll};
use via::err;

pub trait AsyncQueryDsl<T>: RunQueryDsl<T> + Sized {
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

    /// Retrieves an individual row from the databse.
    ///
    /// If no rows are returned, a 403 Forbidden error is returned instead of
    /// 404 Not Found.
    fn fetch_first_async<'a, 'b, U>(
        self,
        connection: &'a mut T,
    ) -> FetchResult<GetResult<'a, 'b, Limit<Self>, T, U>>
    where
        T: AsyncConnectionCore + Send,
        T::Backend: Default,
        <T::Backend as Backend>::QueryBuilder: Default,
        U: Send + 'a,
        Self: LimitDsl,
        <Self as LimitDsl>::Output:
            LoadQuery<'b, T, U> + RunQueryDsl<T> + QueryFragment<T::Backend> + Send + 'b,
    {
        AsyncQueryDsl::fetch_result_async(self.limit(1), connection)
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
        Self: LimitDsl,
        <Self as LimitDsl>::Output:
            LoadQuery<'b, T, U> + RunQueryDsl<T> + QueryFragment<T::Backend> + Send + 'b,
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

    /// Retrieves the result of a database operation without disclosing whether
    /// or not it exists.
    ///
    /// If no rows are returned, a 403 Forbidden error is returned instead of
    /// 404 Not Found.
    fn fetch_result_async<'a, 'b, U>(
        self,
        connection: &'a mut T,
    ) -> FetchResult<GetResult<'a, 'b, Self, T, U>>
    where
        T: AsyncConnectionCore + Send,
        T::Backend: Default,
        <T::Backend as Backend>::QueryBuilder: Default,
        U: Send + 'a,
        Self: LoadQuery<'b, T, U> + QueryFragment<T::Backend> + Send + 'b,
    {
        #[cfg(debug_assertions)]
        debug_query(&self);

        FetchResult {
            future: RunQueryDsl::get_result(self, connection),
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
/// If no rows are returned in the query result set, a 404 status will be used
/// as the error status code.
///
/// [`Error`]: via::Error
pub struct MapErr<F> {
    future: F,
}

/// Load an individual database row.
///
/// If no rows are returned in the query result set, a 403 status will be used
/// as the error status code.
///
/// If no rows are returned in the query result set, a 404 status will be used
/// as the error status code.
///
/// [`Error`]: via::Error
pub struct FetchResult<F> {
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

fn map_err(error: &DieselError, not_found: impl FnOnce() -> via::Error) -> via::Error {
    match error {
        // No rows were returned by a query expected to return at least one row.
        DieselError::NotFound => not_found(),

        // An error was returned by the database.
        DieselError::DatabaseError(kind, info) => {
            err!(status_for_database_error(kind), "{}", info.message())
        }

        // The error occured for some other reason.
        _ => err!(500, "internal server error"),
    }
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

impl<F, T> Future for FetchResult<F>
where
    F: Future<Output = QueryResult<T>> + Unpin,
{
    type Output = via::Result<T>;

    fn poll(mut self: Pin<&mut Self>, context: &mut Context) -> Poll<Self::Output> {
        match Pin::new(&mut self.future).poll(context) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Ok(data)) => Poll::Ready(Ok(data)),
            Poll::Ready(Err(ref error)) => {
                // Placeholder for tracing...
                #[cfg(debug_assertions)]
                eprintln!("error(database): {}", error);

                Poll::Ready(Err(map_err(error, || err!(403, "forbidden"))))
            }
        }
    }
}

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
                // Placeholder for tracing...
                #[cfg(debug_assertions)]
                eprintln!("error(database): {}", error);
                Poll::Ready(Err(map_err(error, || err!(404, "not found"))))
            }
        }
    }
}

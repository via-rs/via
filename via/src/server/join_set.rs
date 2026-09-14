use std::sync::Arc;
use tokio::sync::{OnceCell, mpsc};
use tokio::task::{self, coop};
use tokio::time::{Duration, Instant, error::Elapsed, timeout, timeout_at};

use super::DEFAULT_SHUTDOWN_TIMEOUT;
use crate::error::ServerError;

#[cfg(all(debug_assertions, any(feature = "native-tls", feature = "rustls-23")))]
const MAX_TASK_SIZE: usize = 2048;

#[cfg(all(
    debug_assertions,
    not(any(feature = "native-tls", feature = "rustls-23"))
))]
const MAX_TASK_SIZE: usize = 1024;

pub const COHORT_SIZE: usize = 512;

pub type Sender = mpsc::Sender<Cohort>;
pub type TaskResult = std::result::Result<(), ServerError>;

pub struct Cohort {
    is_dirty: bool,
    tasks: tokio::task::JoinSet<TaskResult>,
}

pub struct JoinSet {
    current: Cohort,
    next: mpsc::Receiver<Cohort>,
}

#[derive(Clone)]
pub struct StartedAt {
    value: Arc<OnceCell<Instant>>,
}

async fn join_connections(is_cooperative: bool, cohort: &mut Cohort) {
    while let Some(result) = cohort.join_next().await {
        if let Err(ref error) = result {
            log!(error(cohort = 0), "{}", error);
        }

        if is_cooperative {
            coop::consume_budget().await;
        }
    }
}

async fn join_cohort(started_at: StartedAt, recycler: Sender, mut cohort: Cohort) {
    log!(info(cohort = 0), "joining {} connections.", cohort.size());

    let future = started_at.timeout(
        DEFAULT_SHUTDOWN_TIMEOUT,
        join_connections(true, &mut cohort),
    );

    if future.await.is_err() {
        if cohort.is_dirty {
            // Tasks that survive more than one cohort generation are detached.
            //
            // This allows locality to drift by not retaining references to
            // persistent connections or join handles to persistent connection
            // tasks.
            //
            // Something that we would do for connections that use a web socket
            // if we were able to tell ahead of time in `accept`.
            cohort.detach_all();
        } else {
            cohort.is_dirty = true;
        }
    }

    if let Err(error) = recycler.try_send(cohort) {
        let mut cohort = error.into_inner();

        // Placeholder for tracing...
        log!(error(cohort = 1), "cohort cannot be recycled.");

        // If the cohort contains connections that could not be joined, detach.
        if cohort.is_dirty {
            log!(
                error(cohort = 2),
                "detaching {} connections.",
                cohort.size()
            );

            cohort.detach_all();
        }
    }
}

impl Cohort {
    fn new() -> Self {
        Self {
            is_dirty: false,
            tasks: Default::default(),
        }
    }

    fn size(&self) -> usize {
        self.tasks.len()
    }

    fn spawn<F>(&mut self, connection: F)
    where
        F: Future<Output = TaskResult> + Send + 'static,
    {
        log!(
            info(cohort = 0),
            "spawn connection task (size = {}).",
            std::mem::size_of_val(&connection)
        );

        // Connections are polled inline. The task dependencies are allocated
        // on the heap.
        //
        // This keeps the cost of joining a connection relatively low while
        // allowing each component to benefit from CPU cache locality when a
        // boxed stream or sink is in the hot path of the state machine.
        #[cfg(debug_assertions)]
        assert!(
            std::mem::size_of_val(&connection) < MAX_TASK_SIZE,
            "connection task size limit of {} exceeded.",
            MAX_TASK_SIZE,
        );

        self.tasks.spawn(connection);
    }

    async fn join_next(&mut self) -> Option<TaskResult> {
        match self.tasks.join_next().await {
            Some(Ok(result)) => Some(result),
            Some(Err(error)) => Some(Err(ServerError::Join(error))),
            None => {
                self.is_dirty = false;
                None
            }
        }
    }

    fn detach_all(&mut self) {
        self.tasks.detach_all();
        self.is_dirty = false;
    }
}

impl JoinSet {
    pub(super) fn new() -> (Sender, Self) {
        let (tx, next) = mpsc::channel(1);
        let join_set = Self {
            current: Cohort::new(),
            next,
        };

        // Seed the next cohort to avoid a load-based allocator signal.
        if tx.try_send(Cohort::new()).is_err() {
            unreachable!();
        }

        (tx, join_set)
    }

    pub(super) fn spawn<F>(&mut self, connection: F)
    where
        F: Future<Output = TaskResult> + Send + 'static,
    {
        // Spawn the task in the current cohort. Dynamic allocations may occur.
        self.current.spawn(connection);
    }

    pub(super) fn rotate(&mut self, started_at: StartedAt, recycler: Sender) {
        // Recycle an cohort or create a new one.
        // This dissociates load from the allocation in `Cohort::new()`.
        let mut next = self.next.try_recv().unwrap_or_else(|_| Cohort::new());

        // Swap the current cohort with the next cohort.
        std::mem::swap(&mut self.current, &mut next);

        // Spawn a detached task `join_cohort` task.
        task::spawn(join_cohort(started_at, recycler, next));
    }

    #[inline]
    pub(super) fn size(&self) -> usize {
        self.current.size()
    }

    pub(super) async fn join(mut self, timeout_after: Duration, recycler: Sender) -> TaskResult {
        let join_primary = join_connections(false, &mut self.current);

        if timeout(timeout_after, join_primary).await.is_err() {
            return Err(ServerError::ShutdownTimeout);
        }

        while let Ok(mut cohort) = self.next.try_recv() {
            let join_rollover = join_connections(false, &mut cohort);
            if timeout(timeout_after, join_rollover).await.is_err() {
                return Err(ServerError::ShutdownTimeout);
            }
        }

        // Keep recycler live until rollover cohorts are joined.
        let _recycler = recycler;

        Ok(())
    }
}

impl StartedAt {
    pub fn new() -> Self {
        Self {
            value: Arc::new(OnceCell::new()),
        }
    }

    pub async fn timeout<F>(self, duration: Duration, future: F) -> Result<F::Output, Elapsed>
    where
        F: Future + Send,
    {
        let get_or_init = self.value.get_or_init(|| async { Instant::now() });
        timeout_at(*coop::unconstrained(get_or_init).await + duration, future).await
    }
}

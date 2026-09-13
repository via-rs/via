use std::sync::Arc;
use tokio::sync::mpsc::error::TryRecvError;
use tokio::sync::{OnceCell, mpsc};
use tokio::task::{self, coop};
use tokio::time::error::Elapsed;
use tokio::time::{Duration, Instant, timeout, timeout_at};

use super::DEFAULT_SHUTDOWN_TIMEOUT;
use crate::error::ServerError;

const COHORT_SIZE: usize = 499;

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
        if let Err(error) = result {
            log!(error(cohort = 1), "(connection) -> {}", &error);
        }

        if is_cooperative {
            coop::consume_budget().await;
        }
    }
}

async fn join_cohort(started_at: StartedAt, recycler: Sender, mut cohort: Cohort) {
    log!(info(cohort = 0), "joining {} connections.", cohort.size());

    let future = started_at.timeout_in(
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

    fn spawn(&mut self, task: impl Future<Output = TaskResult> + Send + 'static) {
        self.tasks.spawn(task);
    }

    async fn join_next(&mut self) -> Option<TaskResult> {
        let joined = self.tasks.join_next().await;
        let joined = joined.and_then(|result| match result {
            Ok(result) => Some(result),
            Err(error) => {
                log!(info(cohort = 1), "(task) -> {}", &error);
                None
            }
        });

        if joined.is_none() {
            self.is_dirty = false;
        }

        joined
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

        (tx, join_set)
    }

    pub(super) fn spawn(
        &mut self,
        started_at: &StartedAt,
        recycler: &Sender,
        task: impl Future<Output = TaskResult> + Send + 'static,
    ) {
        // Spawn the task in the current cohort. Dynamic allocations may occur.
        self.current.spawn(task);

        // If the current cohort exceeds `COHORT_SIZE`, start join it.
        if self.current.size() > COHORT_SIZE {
            // Clone `recycler` to make `TryRecvError::Disconnected` unreachable.
            let recycler = recycler.clone();

            // Clone `started_at` so it can move into the `join_cohort` task.
            let started_at = started_at.clone();

            // Recycle an cohort or create a new one.
            // This dissociates load from the allocation in `Cohort::new()`.
            let mut next_cohort = match self.next.try_recv() {
                // Ideally we always have a cohort ready.
                Ok(cohort) => cohort,
                // There isn't a cohort available to recycle.
                Err(TryRecvError::Empty) => Cohort::new(),
                // Sender is an owned stack variable. This is unreachable.
                Err(TryRecvError::Disconnected) => unreachable!(),
            };

            // Swap the current cohort with the next cohort.
            std::mem::swap(&mut self.current, &mut next_cohort);

            // Spawn a detached task `join_cohort` task.
            task::spawn(async {
                join_cohort(started_at, recycler, next_cohort).await;
            });
        }
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

    pub async fn timeout_in<F>(self, duration: Duration, future: F) -> Result<F::Output, Elapsed>
    where
        F: Future + Send,
    {
        let get_or_init = self.value.get_or_init(|| async { Instant::now() });
        timeout_at(*coop::unconstrained(get_or_init).await + duration, future).await
    }
}

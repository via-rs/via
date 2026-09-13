use std::sync::Arc;
use tokio::sync::mpsc::error::TryRecvError;
use tokio::sync::{OnceCell, mpsc};
use tokio::task::{self, JoinError, coop};
use tokio::time::{Duration, Instant, Timeout};

use crate::error::ServerError;

const COHORT_SIZE: usize = u8::MAX as usize;

type JoinResult = std::result::Result<Result, JoinError>;

pub type Result = std::result::Result<(), ServerError>;
pub type Sender = mpsc::Sender<Cohort>;

pub struct Cohort {
    is_dirty: bool,
    tasks: tokio::task::JoinSet<Result>,
}

pub struct JoinSet {
    current: Cohort,
    next: mpsc::Receiver<Cohort>,
}

#[derive(Clone)]
pub struct StartedAt {
    value: Arc<OnceCell<Instant>>,
}

struct JoinContext {
    is_cooperative: bool,
    timeout_after: Duration,
    started_at: StartedAt,
    recycler: Sender,
}

async fn join_cohort(mut cohort: Cohort, context: JoinContext) {
    log!(info(cohort = 0), "joining {} connections.", cohort.size());

    let future = async {
        while let Some(result) = cohort.join_next().await {
            #[cfg(not(debug_assertions))]
            drop(result);

            #[cfg(debug_assertions)]
            match result {
                Ok(Ok(_)) => {}
                Err(error) => {
                    log!(error(cohort = 1), "(connection) -> {}", &error);
                }
                Ok(Err(error)) => {
                    log!(error(cohort = 1), "(service) -> {}", &error);
                }
            }

            if context.is_cooperative {
                coop::consume_budget().await;
            }
        }
    };

    let future = context
        .started_at
        .timeout_in(context.timeout_after, future)
        .await;

    if future.await.is_err() {
        if cohort.is_dirty {
            if cohort.size() == 1 {
                cohort.tasks.abort_all();
            } else {
                cohort.tasks.detach_all();
            }

            cohort.is_dirty = false;
        } else {
            cohort.is_dirty = true;
        }
    }

    if context.is_cooperative && context.recycler.try_send(cohort).is_err() {
        // Placeholder for tracing...
        log!(error(cohort = 2), "cohort cannot be recycled.");
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

    fn spawn(&mut self, task: impl Future<Output = Result> + Send + 'static) {
        self.tasks.spawn(task);
    }

    fn join_next(&mut self) -> impl Future<Output = Option<JoinResult>> {
        self.tasks.join_next()
    }
}

impl JoinSet {
    pub(super) fn new() -> (Sender, Self) {
        let (tx, next) = mpsc::channel(2);
        let join_set = Self {
            current: Cohort::new(),
            next,
        };

        (tx, join_set)
    }

    pub(super) fn spawn(
        &mut self,
        started_at: &StartedAt,
        sender: &Sender,
        task: impl Future<Output = Result> + Send + 'static,
    ) {
        // Spawn the task in the current cohort. Dynamic allocations may occur.
        self.current.spawn(task);

        // If the current cohort exceeds `COHORT_SIZE`, start join it.
        if self.current.size() > COHORT_SIZE {
            // Clone `sender` to make `TryRecvError::Disconnected` unreachable.
            let sender = sender.clone();

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

            // Group the dependencies of the `join_cohort` task in a struct.
            let join_context = JoinContext::new(Duration::from_secs(10), started_at, sender);

            // Spawn a detached task `join_cohort` task.
            task::spawn(join_cohort(next_cohort, join_context));
        }
    }

    pub(super) fn join(self, timeout_after: Duration, recycler: Sender) -> impl Future {
        let context = JoinContext::shutdown(timeout_after, recycler);
        join_cohort(self.current, context)
    }
}

impl JoinContext {
    fn new(timeout_after: Duration, started_at: StartedAt, recycler: Sender) -> Self {
        Self {
            is_cooperative: true,
            timeout_after,
            started_at,
            recycler,
        }
    }

    fn shutdown(timeout_after: Duration, recycler: Sender) -> Self {
        let started_at = StartedAt::new();

        Self {
            is_cooperative: false,
            timeout_after,
            started_at,
            recycler,
        }
    }
}

impl StartedAt {
    pub fn new() -> Self {
        Self {
            value: Arc::new(OnceCell::new()),
        }
    }

    pub fn timeout_in<F>(self, duration: Duration, future: F) -> impl Future<Output = Timeout<F>>
    where
        F: Future + Send,
    {
        coop::unconstrained(async move {
            let now = self.value.get_or_init(|| async { Instant::now() }).await;
            let deadline = *now + duration;

            tokio::time::timeout_at(deadline, future)
        })
    }
}

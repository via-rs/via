use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task::coop::unconstrained;
use tokio::task::{self, JoinError, coop};
use tokio::time::timeout;

const JOIN_DEADLINE: Duration = super::MAX_SHUTDOWN_TIMEOUT;

pub type Sender = mpsc::Sender<Cohort>;
pub type TaskResult = std::result::Result<(), JoinError>;

pub struct Cohort {
    is_dirty: bool,
    tasks: tokio::task::JoinSet<()>,
}

pub struct JoinSet {
    current: Cohort,
    next: mpsc::Receiver<Cohort>,
}

async fn join_cohort(recycler: Sender, mut cohort: Cohort) {
    log!(info(cohort = 0), "joining {} connections", cohort.size());

    // Tasks that survive more than one cohort generation are detached.
    if timeout(JOIN_DEADLINE, cohort.join_all()).await.is_ok() {
        log!(info(cohort = 1), "cohort drained successfully");
        cohort.is_dirty = false;
    } else if cohort.is_dirty {
        log!(info(cohort = 1), "detaching {} connections", cohort.size());
        cohort.tasks.detach_all();
        cohort.is_dirty = false;
    } else {
        log!(info(cohort = 1), "{} connections remain", cohort.size());
        cohort.is_dirty = true;
    }

    if recycler.try_send(cohort).is_err() {
        log!(warn(cohort = 1), "cohort cannot be recycled");
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
        F: Future<Output = ()> + Send + 'static,
    {
        #[cfg(debug_assertions)]
        crate::util::once!(|| {
            log!(
                info(cohort = 0),
                "connection task size = {}",
                std::mem::size_of_val(&connection)
            );
        });

        self.tasks.spawn(connection);
    }

    #[inline]
    fn join_next(&mut self) -> impl Future<Output = Option<TaskResult>> {
        self.tasks.join_next()
    }

    async fn join_all(&mut self) {
        while let Some(result) = self.join_next().await {
            if let Err(error) = result {
                log!(error(connection = 0), "{}", &error);
            }

            coop::consume_budget().await;
        }
    }
}

impl JoinSet {
    pub(super) fn new(capacity: usize) -> (Sender, Self) {
        let (tx, next) = mpsc::channel(capacity);
        let join_set = Self {
            current: Cohort::new(),
            next,
        };

        (tx, join_set)
    }

    pub(super) fn spawn<F>(&mut self, connection: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        // Spawn the task in the current cohort. Dynamic allocations may occur.
        self.current.spawn(connection);
    }

    pub(super) fn rotate(&mut self, recycler: Sender) {
        // Recycle a cohort or create a new one.
        // This decorrelates load from the allocation in `Cohort::new()`.
        let mut next = self.next.try_recv().unwrap_or_else(|_| {
            log!(info(cohort = 0), "allocation required for rotation");
            Cohort::new()
        });

        // Swap the current cohort with the next cohort.
        std::mem::swap(&mut self.current, &mut next);

        // Spawn a detached task `join_cohort`.
        task::spawn(join_cohort(recycler, next));
    }

    #[inline]
    pub(super) fn size(&self) -> usize {
        self.current.size()
    }

    pub(super) async fn join_all(self) {
        let mut next = Some(self.current);
        let mut rx = self.next;

        while let Some(mut cohort) = next {
            next = rx.try_recv().ok();
            unconstrained(cohort.join_all()).await;
        }
    }
}

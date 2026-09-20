use tokio::sync::mpsc;
use tokio::task::{self, JoinError, coop};
use tokio::time::timeout;

use super::DEFAULT_SHUTDOWN_TIMEOUT;

pub const COHORT_SIZE: usize = 512;

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

async fn join_connections(is_cooperative: bool, cohort: &mut Cohort) {
    while let Some(result) = cohort.join_next().await {
        if let Err(ref error) = result {
            log!(error(cohort = 0), "{}", error);
        }

        if is_cooperative {
            coop::consume_budget().await;
        }
    }

    cohort.is_dirty = false;
}

async fn join_cohort(recycler: Sender, mut cohort: Cohort) {
    log!(info(cohort = 0), "joining {} connections.", cohort.size());

    let future = timeout(
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
        F: Future<Output = ()> + Send + 'static,
    {
        #[cfg(debug_assertions)]
        crate::util::once!(|| {
            let size = std::mem::size_of_val(&connection);
            println!("connection task size = {}", size);
        });

        self.tasks.spawn(connection);
    }

    #[inline]
    fn join_next(&mut self) -> impl Future<Output = Option<TaskResult>> {
        self.tasks.join_next()
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
        F: Future<Output = ()> + Send + 'static,
    {
        // Spawn the task in the current cohort. Dynamic allocations may occur.
        self.current.spawn(connection);
    }

    pub(super) fn rotate(&mut self, recycler: Sender) {
        // Recycle a cohort or create a new one.
        // This dissociates load from the allocation in `Cohort::new()`.
        let mut next = self.next.try_recv().unwrap_or_else(|_| Cohort::new());

        // Swap the current cohort with the next cohort.
        std::mem::swap(&mut self.current, &mut next);

        // Spawn a detached task `join_cohort`.
        task::spawn(join_cohort(recycler, next));
    }

    #[inline]
    pub(super) fn size(&self) -> usize {
        self.current.size()
    }

    pub(super) async fn join_all(mut self) {
        join_connections(false, &mut self.current).await;
        while let Ok(mut cohort) = self.next.try_recv() {
            join_connections(false, &mut cohort).await;
        }
    }
}

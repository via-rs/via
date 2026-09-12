use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TryRecvError;
use tokio::task::{self, JoinError, coop};

use crate::error::ServerError;

const COHORT_SIZE: usize = 999;

type TaskResult = std::result::Result<(), ServerError>;
pub type Sender = mpsc::Sender<Cohort>;

pub struct Cohort(tokio::task::JoinSet<TaskResult>);

pub struct JoinSet {
    current: Cohort,
    next: mpsc::Receiver<Cohort>,
}

async fn join_cohort(immediate: bool, tx: Sender, mut cohort: Cohort) {
    log!(info(cohort = 0), "joining {} connections.", cohort.size());

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

        if !immediate {
            coop::consume_budget().await;
        }
    }

    if tx.try_send(cohort).is_err() {
        // Placeholder for tracing...
        log!(error(cohort = 2), "cohort cannot be recycled.");
    }
}

impl Cohort {
    fn new() -> Self {
        Self(Default::default())
    }

    fn size(&self) -> usize {
        self.0.len()
    }

    fn spawn(&mut self, task: impl Future<Output = TaskResult> + Send + 'static) {
        self.0.spawn(task);
    }

    fn join_next(&mut self) -> impl Future<Output = Option<Result<TaskResult, JoinError>>> {
        self.0.join_next()
    }
}

impl JoinSet {
    pub(super) fn new() -> (Sender, Self) {
        let (tx, next) = mpsc::channel(1);
        let join_set = Self {
            current: Cohort::new(),
            next,
        };

        // We have exclusive access to `next` and we know that it is empty.
        if tx.try_send(Cohort::new()).is_err() {
            unreachable!();
        }

        (tx, join_set)
    }

    pub(super) fn spawn(
        &mut self,
        sender: &Sender,
        task: impl Future<Output = TaskResult> + Send + 'static,
    ) {
        // Spawn the task in the current cohort. Dynamic allocations may occur.
        self.current.spawn(task);

        // If the current cohort exceeds `COHORT_SIZE`, start join it.
        if self.current.size() > COHORT_SIZE {
            // Clone sender first, it makes `TryRecvError::Disconnected` truly
            // unreachable.
            let sender = sender.clone();

            // Recycle an cohort or create a new one.
            // This dissociates load from allocations in a hot path.
            let mut next = match self.next.try_recv() {
                // Ideally we always have a cohort ready.
                // This lowers the likelyhood of reallocating in `spawn`.
                Ok(cohort) => cohort,
                // There isn't a cohort available to recycle.
                Err(TryRecvError::Empty) => Cohort::new(),
                // Sender is an owned stack variable. This is unreachable.
                Err(TryRecvError::Disconnected) => unreachable!(),
            };

            // Swap the current cohort with the next cohort.
            std::mem::swap(&mut self.current, &mut next);

            // Spawn a detached task `join_cohort` task.
            task::spawn(join_cohort(false, sender, next));
        }
    }

    pub(super) fn join(self, sender: Sender) -> impl Future {
        join_cohort(true, sender, self.current)
    }
}

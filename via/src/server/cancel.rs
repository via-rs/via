use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Notify;

#[derive(Clone)]
pub struct Cancellation(Arc<Inner>);

struct Inner {
    cancelled: AtomicBool,
    notify: Notify,
}

impl Cancellation {
    pub fn new() -> Self {
        let token = Arc::new(Inner {
            cancelled: AtomicBool::new(false),
            notify: Notify::new(),
        });

        tokio::spawn({
            let token = Arc::clone(&token);
            let ctrl_c = Box::pin(async {
                if tokio::signal::ctrl_c().await.is_err() {
                    eprintln!("unable to register the 'ctrl-c' signal.");
                }
            });

            async move {
                ctrl_c.await;

                let token = &*token;

                token.cancelled.store(true, Ordering::SeqCst);
                token.notify.notify_waiters();
            }
        });

        Self(token)
    }

    pub async fn wait(&self) {
        if !self.0.cancelled.load(Ordering::SeqCst) {
            self.0.notify.notified().await;
        }
    }
}

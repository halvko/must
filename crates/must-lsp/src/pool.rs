//! A small fixed-size worker pool for request handling. Workers exit when
//! the pool (and thus the channel sender) is dropped; a task that panics
//! is caught and logged, and its worker goes on to the next task.

type Task = Box<dyn FnOnce() + Send>;

/// Workers run analysis and const evaluation, and const-forcing recursion
/// burns Rust stack per nested item — with the 2 MiB default, a deep const
/// chain overflows before eval's 128-level forcing cap fires, and a stack
/// overflow kills the whole server (it cannot be caught). The cap's budget
/// assumes this size.
const WORKER_STACK_SIZE: usize = 8 * 1024 * 1024;

pub(crate) struct TaskPool {
    sender: crossbeam_channel::Sender<Task>,
}

impl TaskPool {
    pub(crate) fn new(threads: usize) -> TaskPool {
        let (sender, receiver) = crossbeam_channel::unbounded::<Task>();
        for i in 0..threads {
            let receiver = receiver.clone();
            std::thread::Builder::new()
                .name(format!("must-lsp-worker-{i}"))
                .stack_size(WORKER_STACK_SIZE)
                .spawn(move || {
                    while let Ok(task) = receiver.recv() {
                        // A panicking task must not kill the worker: with a
                        // fixed pool, dead workers silently shrink it until
                        // the server stops answering anything. Request
                        // handlers additionally answer `InternalError`
                        // themselves (see `spawn_request`).
                        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(task)).is_err() {
                            tracing::error!("worker task panicked");
                        }
                    }
                })
                .expect("failed to spawn worker thread");
        }
        TaskPool { sender }
    }

    pub(crate) fn spawn(&self, task: impl FnOnce() + Send + 'static) {
        let _ = self.sender.send(Box::new(task));
    }
}

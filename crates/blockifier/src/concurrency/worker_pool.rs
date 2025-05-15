use std::panic;
use std::rc::Rc; // TODO: remove
use std::sync::{mpsc, Arc};

use crate::blockifier::config::ConcurrencyConfig;
use crate::concurrency::utils::AbortIfPanic;
use crate::concurrency::worker_logic::WorkerExecutor;
use crate::state::state_api::StateReader;

pub struct WorkerPool<S: StateReader> {
    stack_size: usize,
    concurrency_config: ConcurrencyConfig,
    channels: Vec<mpsc::Sender<Option<Arc<WorkerExecutor<S>>>>>,
}

impl<S: StateReader> WorkerPool<S> {
    // TODO: document.
    pub fn run(stack_size: usize, concurrency_config: ConcurrencyConfig, worker_executor: Arc<WorkerExecutor<S>>)
    where S: 'static
     { // TODO: remove worker_executor
        // Initialize the channels.
        let mut senders = Vec::<mpsc::Sender<Option<Arc<WorkerExecutor<S>>>>>::new();
        let mut receivers = Vec::<mpsc::Receiver<Option<Arc<WorkerExecutor<S>>>>>::new();
        for _ in 0..concurrency_config.n_workers {
            let (sender, receiver) = mpsc::channel();
            senders.push(sender);
            receivers.push(receiver);
        }

        let receiver: mpsc::Receiver<S> =  mpsc::channel().1;

        std::thread::Builder::new().spawn(move || {
            dbg!(receiver);
        });

        std::thread::scope();

        // Run the threads.

        // TODO: fix comment.
        // No thread pool implementation is needed here since we already have our scheduler. The
        // initialized threads below will "busy wait" for new tasks using the `run` method until the
        // chunk execution is completed, and then they will be joined together in a for loop.
        // TODO(barak, 01/07/2024): Consider using tokio and spawn tasks that will be served by some
        // upper level tokio thread pool (Runtime in tokio terminology).
        for receiver in receivers.into_iter() {
            let func = move || {
                let r = Arc::new(receiver);
                // dbg!(receiver);
                // WorkerPool::run_thread(receiver)
            };
            std::thread::Builder::new()
                // when running Cairo natively, the real stack is used and could get overflowed
                // (unlike the VM where the stack is simulated in the heap as a memory segment).
                //
                // We pre-allocate the stack here, and not during Native execution (not trivial), so it
                // needs to be big enough ahead.
                // However, making it very big is wasteful (especially with multi-threading).
                // So, the stack size should support calls with a reasonable gas limit, for extremely deep
                // recursions to reach out-of-gas before hitting the bottom of the recursion.
                //
                // The gas upper bound is MAX_POSSIBLE_SIERRA_GAS, and sequencers must not raise it without
                // adjusting the stack size.
                .stack_size(stack_size)
                .spawn(func)
                .expect("Failed to spawn thread.");
        }

        for sender in senders {
            sender.send(Some(worker_executor.clone()));
        }
    }

    fn run_thread(receiver: mpsc::Receiver<Option<Arc<WorkerExecutor<S>>>>) {
        // Fetch worker executor from the channel, until None is received.
        while let Some(worker_executor) =
            receiver.recv().expect("Failed to receive worker executor.")
        {
            WorkerPool::run_executor(&*worker_executor);
        }
    }

    fn run_executor(worker_executor: &WorkerExecutor<S>) {
        // Making sure that the program will abort if a panic occured while halting
        // the scheduler.
        let abort_guard = AbortIfPanic;
        // If a panic is not handled or the handling logic itself panics, then we
        // abort the program.
        let res = panic::catch_unwind(panic::AssertUnwindSafe(|| { worker_executor.run(); }));
        if let Err(err) = res {
            // If the program panics here, the abort guard will exit the program.
            // In this case, no panic message will be logged. Add the cargo flag
            // --nocapture to log the panic message.

            worker_executor.scheduler.halt();
            abort_guard.release();
            panic::resume_unwind(err);
        }

        abort_guard.release();
    }
}

use std::panic;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};

use crate::blockifier::config::ConcurrencyConfig;
use crate::concurrency::utils::AbortIfPanic;
use crate::concurrency::worker_logic::WorkerExecutor;
use crate::state::state_api::StateReader;

/// Used to execute transactions concurrently.
/// Call `run()` to start executing a chunk of transactions (represented by a [WorkerExecutor]).
/// Call `join()` to wait for all the threads to finish.
///
/// If an execution of a chunk is halted (`Scheduler::halt`), each thread will continue to run until
/// finishing the current execution (excluding reruns), and then move to the next chunk.
/// The transactions that were not fully executed by the time halt was called will be discarded.
#[derive(Debug)]
pub struct WorkerPool<S: StateReader> {
    senders: Vec<mpsc::Sender<Option<Arc<WorkerExecutor<S>>>>>,
    handlers: Vec<std::thread::JoinHandle<()>>,
    /// Whether one of the threads panicked.
    a_thread_panicked: Arc<AtomicBool>,
}

impl<S: StateReader + Send + 'static> WorkerPool<S> {
    /// Creates a new WorkerPool with the given stack size and concurrency configuration.
    pub fn start(stack_size: usize, concurrency_config: ConcurrencyConfig) -> Self {
        // Initialize the channels.
        let a_thread_panicked = Arc::new(AtomicBool::new(false));
        let mut senders = Vec::<mpsc::Sender<Option<Arc<WorkerExecutor<S>>>>>::new();
        let mut receivers = Vec::<mpsc::Receiver<Option<Arc<WorkerExecutor<S>>>>>::new();
        for _ in 0..concurrency_config.n_workers {
            let (sender, receiver) = mpsc::channel();
            senders.push(sender);
            receivers.push(receiver);
        }

        // Run the threads.
        let handlers = receivers
            .into_iter()
            .map(|receiver| {
                let mut thread_builder = std::thread::Builder::new();
                // When running Cairo natively, the real stack is used and could get overflowed
                // (unlike the VM where the stack is simulated in the heap as a memory segment).
                //
                // We pre-allocate the stack here, and not during Native execution (not trivial), so
                // it needs to be big enough ahead.
                // However, making it very big is wasteful (especially with multi-threading).
                // So, the stack size should support calls with a reasonable gas limit, for
                // extremely deep recursions to reach out-of-gas before hitting the
                // bottom of the recursion.
                //
                // The gas upper bound is MAX_POSSIBLE_SIERRA_GAS, and sequencers must not raise it
                // without adjusting the stack size.
                thread_builder = thread_builder.stack_size(stack_size);
                let worker_thread = WorkerThread {
                    a_thread_panicked: a_thread_panicked.clone(),
                    receiver,
                };
                thread_builder
                    .spawn(move || worker_thread.run_thread())
                    .expect("Failed to spawn thread.")
            })
            .collect();

        WorkerPool { senders, handlers, a_thread_panicked }
    }

    pub fn run(&self, worker_executor: Arc<WorkerExecutor<S>>) {
        for sender in self.senders.iter() {
            sender.send(Some(worker_executor.clone())).expect("Failed to send worker executor.");
        }
        worker_executor.scheduler.wait_for_completion();

        if self.a_thread_panicked.load(Ordering::Acquire) {
            panic!("One of the threads panicked.");
        }
    }

    pub fn join(self) {
        // Send None to all senders to stop the threads.
        for sender in self.senders {
            sender.send(None).expect("Failed to signal worker thread to stop.");
        }
        for handler in self.handlers {
            handler.join().expect("Failed to join thread.");
        }
    }
}

struct WorkerThread<S: StateReader> {
    a_thread_panicked: Arc<AtomicBool>,
    receiver: mpsc::Receiver<Option<Arc<WorkerExecutor<S>>>>,
}

impl<S: StateReader> WorkerThread<S> {
    /// Fetches worker executors from the channel, until None is received.
    fn run_thread(&self) {
        while let Some(worker_executor) =
            self.receiver.recv().expect("Failed to receive worker executor.")
        {
            self._run_executor(&*worker_executor);
        }
    }

    /// Runs a single worker executor.
    fn _run_executor(&self, worker_executor: &WorkerExecutor<S>) {
        if self.a_thread_panicked.load(Ordering::Acquire) {
            panic!("Another thread panicked. Aborting.");
        }

        // Making sure that the program will abort if a panic occured while halting
        // the scheduler.
        let abort_guard = AbortIfPanic;
        // If a panic is not handled or the handling logic itself panics, then we
        // abort the program.
        let res = panic::catch_unwind(panic::AssertUnwindSafe(|| {
            worker_executor.run();
        }));
        if let Err(err) = res {
            // First, set the panic flag. This must be done before halting the scheduler.
            self.a_thread_panicked.store(true, Ordering::Release);

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

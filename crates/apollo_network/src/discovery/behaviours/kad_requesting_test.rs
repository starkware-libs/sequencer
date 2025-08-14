use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll, Waker};

use futures::StreamExt;
use libp2p::swarm::SwarmEvent;
use libp2p::Swarm;
use libp2p_swarm_test::SwarmExt;
use tokio::time::Duration;
use waker_fn::waker_fn;

use super::kad_requesting::KadRequestingBehaviour;
use crate::discovery::ToOtherBehaviourEvent;

const SYNC_ATOMIC_ORDERING: Ordering = Ordering::SeqCst;

struct WakerCountingWrapper {
    times_woken: Arc<AtomicUsize>,
    waker: Waker,
}

impl WakerCountingWrapper {
    pub fn new() -> Self {
        let times_woken = Arc::new(AtomicUsize::new(0));
        let times_woken_clone = times_woken.clone();
        let waker = waker_fn(move || {
            times_woken_clone.fetch_add(1, SYNC_ATOMIC_ORDERING);
        });
        Self { times_woken, waker }
    }

    pub fn create_context(&self) -> Context<'_> {
        Context::from_waker(&self.waker)
    }

    pub fn times_woken(&self) -> usize {
        self.times_woken.load(SYNC_ATOMIC_ORDERING)
    }
}

/// Manually polls the swarm using a (new) counting waker and returns the result of the poll and the
/// waker wrapper.
fn poll_swarm(
    swarm: &mut Swarm<KadRequestingBehaviour>,
) -> (Poll<Option<SwarmEvent<ToOtherBehaviourEvent>>>, WakerCountingWrapper) {
    let counting_waker = WakerCountingWrapper::new();
    let mut cx = counting_waker.create_context();

    (swarm.poll_next_unpin(&mut cx), counting_waker)
}

pub fn create_kad_requesting_swarm(heartbeat_interval: Duration) -> Swarm<KadRequestingBehaviour> {
    Swarm::new_ephemeral_tokio(move |_| KadRequestingBehaviour::new(heartbeat_interval))
}

const HEARBEAT_INTERVAL: Duration = Duration::from_secs(5);

#[tokio::test(start_paused = true)]
async fn sends_query_immediately_on_creation() {
    let mut swarm = create_kad_requesting_swarm(HEARBEAT_INTERVAL);

    // Time is stopped, we haven't advanced it at all since creating the behavior still we expect a
    // query event.
    let (poll_res, waker) = poll_swarm(&mut swarm);
    assert!(matches!(
        poll_res,
        Poll::Ready(Some(SwarmEvent::Behaviour(ToOtherBehaviourEvent::RequestKadQuery(_))))
    ));
    assert_eq!(waker.times_woken(), 0);
}

#[tokio::test(start_paused = true)]
async fn awakes_waker_when_time_to_query() {
    const DELTA: Duration = Duration::from_millis(10);

    let mut swarm = create_kad_requesting_swarm(HEARBEAT_INTERVAL);

    // Kad requesting sends the *first* request immediately when called.
    // This behaviour is verified in its own test so we don't check the returned values here.
    let _ = poll_swarm(&mut swarm);

    // Next poll should occur at `now` + `HEARBEAT_INTERVAL`. We advance the time to not reach it
    // yet.
    tokio::time::advance(HEARBEAT_INTERVAL - DELTA).await;

    let (poll_res, waker) = poll_swarm(&mut swarm);
    assert!(matches!(poll_res, Poll::Pending));
    // Make sure to yield so we know the reason the waker was not woken is *not* because the tokio
    // task didn't get a chance to run.
    tokio::task::yield_now().await;
    assert_eq!(waker.times_woken(), 0);

    // Advance the time to beyond the next heartbeat. This should cause the waker to be woken.
    tokio::time::advance(2 * DELTA).await;
    // We must yield now or else the task that needs to call awake will not run.
    tokio::task::yield_now().await;

    assert_eq!(waker.times_woken(), 1);

    // Now that we were woken, calling poll again should return a new event.
    let (poll_res, waker) = poll_swarm(&mut swarm);
    assert!(matches!(
        poll_res,
        Poll::Ready(Some(SwarmEvent::Behaviour(ToOtherBehaviourEvent::RequestKadQuery(_))))
    ));
    assert_eq!(waker.times_woken(), 0);
}

use crate::server::shutdown::{race_shutdown_against_second_signal, ShutdownOutcome};

// Both futures below are ready on the first poll, so looping makes a
// regression (an unbiased select picking a branch at random) fail with
// overwhelming probability instead of only occasionally.
#[tokio::test]
async fn stopped_wins_when_both_futures_are_ready_immediately() {
    for _ in 0..200 {
        let outcome = race_shutdown_against_second_signal(
            std::future::ready(()),
            std::future::ready("SIGTERM"),
        )
        .await;
        assert_eq!(outcome, ShutdownOutcome::Clean);
    }
}

#[tokio::test]
async fn force_exits_when_only_the_second_signal_is_ready() {
    let outcome =
        race_shutdown_against_second_signal(std::future::pending(), std::future::ready("SIGTERM"))
            .await;
    assert_eq!(outcome, ShutdownOutcome::ForceExit("SIGTERM"));
}

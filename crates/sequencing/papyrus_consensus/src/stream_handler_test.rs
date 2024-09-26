use futures::channel::mpsc;
use papyrus_protobuf::consensus::{ConsensusMessage, Proposal, StreamMessage};

use super::StreamHandler;

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn make_test_message(
        stream_id: u64,
        message_id: u64,
        fin: bool,
    ) -> StreamMessage<ConsensusMessage> {
        StreamMessage {
            message: ConsensusMessage::Proposal(Proposal::default()),
            stream_id,
            message_id,
            fin,
        }
    }

    // Check if two vectors are the same:
    fn do_vecs_match<T: PartialEq>(a: &[T], b: &[T]) -> bool {
        let matching = a.iter().zip(b.iter()).filter(|&(a, b)| a == b).count();
        matching == a.len() && matching == b.len()
    }

    fn setup_test() -> (
        StreamHandler<ConsensusMessage>,
        mpsc::Sender<StreamMessage<ConsensusMessage>>,
        mpsc::Receiver<mpsc::Receiver<ConsensusMessage>>,
    ) {
        let (tx_input, rx_input) = mpsc::channel::<StreamMessage<ConsensusMessage>>(100);
        let (tx_output, rx_output) = mpsc::channel::<mpsc::Receiver<ConsensusMessage>>(100);
        let handler = StreamHandler::new(tx_output, rx_input);
        (handler, tx_input, rx_output)
    }

    #[tokio::test]
    async fn stream_handler_in_order() {
        let (mut stream_handler, mut tx_input, mut rx_output) = setup_test();

        let stream_id = 127;
        for i in 0..10 {
            let message = make_test_message(stream_id, i, i == 9);
            tx_input.try_send(message).expect("Send should succeed");
        }

        let join_handle = tokio::spawn(async move {
            let _ = tokio::time::timeout(Duration::from_millis(100), stream_handler.listen()).await;
        });

        join_handle.await.expect("Task should succeed");

        let mut receiver = rx_output.try_next().unwrap().unwrap();
        for _ in 0..10 {
            let _ = receiver.try_next().unwrap().unwrap();
        }
        // Check that the receiver was closed:
        assert!(matches!(receiver.try_next(), Ok(None)));
    }

    #[tokio::test]
    async fn stream_handler_in_reverse() {
        let (mut stream_handler, mut tx_input, mut rx_output) = setup_test();

        let stream_id = 127;
        for i in 0..5 {
            let message = make_test_message(stream_id, 5 - i, i == 0);
            tx_input.try_send(message).expect("Send should succeed");
        }

        let join_handle = tokio::spawn(async move {
            let _ = tokio::time::timeout(Duration::from_millis(100), stream_handler.listen()).await;
            stream_handler
        });
        let mut stream_handler = join_handle.await.expect("Task should succeed");

        // Get the receiver for the stream.
        let mut receiver = rx_output.try_next().unwrap().unwrap();
        // Check that the channel is empty (no messages were sent yet).
        assert!(receiver.try_next().is_err());

        assert_eq!(stream_handler.stream_data.len(), 1);
        assert_eq!(stream_handler.stream_data[&stream_id].message_buffer.len(), 5);
        let range: Vec<u64> = (1..6).collect();
        let keys: Vec<u64> =
            stream_handler.stream_data[&stream_id].message_buffer.clone().into_keys().collect();
        assert!(do_vecs_match(&keys, &range));

        // Now send the last message:
        tx_input.try_send(make_test_message(stream_id, 0, false)).expect("Send should succeed");

        let join_handle = tokio::spawn(async move {
            let _ = tokio::time::timeout(Duration::from_millis(100), stream_handler.listen()).await;
            stream_handler
        });

        let stream_handler = join_handle.await.expect("Task should succeed");
        assert!(stream_handler.stream_data.is_empty());

        for _ in 0..6 {
            let _ = receiver.try_next().unwrap().unwrap();
        }
        // Check that the receiver was closed:
        assert!(matches!(receiver.try_next(), Ok(None)));
    }

    #[tokio::test]
    async fn stream_handler_multiple_streams() {
        let (mut stream_handler, mut tx_input, mut rx_output) = setup_test();

        let stream_id1 = 127; // Send all messages in order (except the first one).
        let stream_id2 = 10; // Send in reverse order (except the first one).
        let stream_id3 = 1; // Send in two batches of 5 messages, without the first one, don't send fin.

        for i in 1..10 {
            let message = make_test_message(stream_id1, i, i == 9);
            tx_input.try_send(message).expect("Send should succeed");
        }

        for i in 0..5 {
            let message = make_test_message(stream_id2, 5 - i, i == 0);
            tx_input.try_send(message).expect("Send should succeed");
        }

        for i in 5..10 {
            let message = make_test_message(stream_id3, i, false);
            tx_input.try_send(message).expect("Send should succeed");
        }
        for i in 1..5 {
            let message = make_test_message(stream_id3, i, false);
            tx_input.try_send(message).expect("Send should succeed");
        }

        let join_handle = tokio::spawn(async move {
            let _ = tokio::time::timeout(Duration::from_millis(100), stream_handler.listen()).await;
            stream_handler
        });
        let mut stream_handler = join_handle.await.expect("Task should succeed");

        let values = vec![1, 10, 127];
        assert!(stream_handler.stream_data.clone().into_keys().all(|item| values.contains(&item)));

        // We have all message from 1 to 9 buffered.
        assert!(do_vecs_match(
            &stream_handler.stream_data[&stream_id1]
                .message_buffer
                .clone()
                .into_keys()
                .collect::<Vec<_>>(),
            &(1..10).collect::<Vec<_>>()
        ));

        // We have all message from 1 to 5 buffered.
        assert!(do_vecs_match(
            &stream_handler.stream_data[&stream_id2]
                .message_buffer
                .clone()
                .into_keys()
                .collect::<Vec<_>>(),
            &(1..6).collect::<Vec<_>>()
        ));

        // We have all message from 1 to 5 buffered.
        assert!(do_vecs_match(
            &stream_handler.stream_data[&stream_id3]
                .message_buffer
                .clone()
                .into_keys()
                .collect::<Vec<_>>(),
            &(1..10).collect::<Vec<_>>()
        ));

        // Get the receiver for the first stream.
        let mut receiver1 = rx_output.try_next().unwrap().unwrap();

        // Check that the channel is empty (no messages were sent yet).
        assert!(receiver1.try_next().is_err());

        // Get the receiver for the second stream.
        let mut receiver2 = rx_output.try_next().unwrap().unwrap();

        // Check that the channel is empty (no messages were sent yet).
        assert!(receiver2.try_next().is_err());

        // Get the receiver for the third stream.
        let mut receiver3 = rx_output.try_next().unwrap().unwrap();

        // Check that the channel is empty (no messages were sent yet).
        assert!(receiver3.try_next().is_err());

        // Send the last message on stream_id1:
        tx_input.try_send(make_test_message(stream_id1, 0, false)).expect("Send should succeed");
        let join_handle = tokio::spawn(async move {
            let _ = tokio::time::timeout(Duration::from_millis(100), stream_handler.listen()).await;
            stream_handler
        });

        let mut stream_handler = join_handle.await.expect("Task should succeed");

        // Should be able to read all the messages for stream_id1.
        for _ in 0..10 {
            let _ = receiver1.try_next().unwrap().unwrap();
        }

        // Check that the receiver was closed:
        assert!(matches!(receiver1.try_next(), Ok(None)));

        // stream_id1 should be gone
        let values = vec![1, 10];
        assert!(stream_handler.stream_data.clone().into_keys().all(|item| values.contains(&item)));

        // Send the last message on stream_id2:
        tx_input.try_send(make_test_message(stream_id2, 0, false)).expect("Send should succeed");
        let join_handle = tokio::spawn(async move {
            let _ = tokio::time::timeout(Duration::from_millis(100), stream_handler.listen()).await;
            stream_handler
        });

        let mut stream_handler = join_handle.await.expect("Task should succeed");

        // Should be able to read all the messages for stream_id2.
        for _ in 0..6 {
            let _ = receiver2.try_next().unwrap().unwrap();
        }

        // Check that the receiver was closed:
        assert!(matches!(receiver2.try_next(), Ok(None)));

        // Stream_id2 should also be gone.
        let values = vec![1];
        assert!(stream_handler.stream_data.clone().into_keys().all(|item| values.contains(&item)));

        // Send the last message on stream_id3:
        tx_input.try_send(make_test_message(stream_id3, 0, false)).expect("Send should succeed");

        let join_handle = tokio::spawn(async move {
            let _ = tokio::time::timeout(Duration::from_millis(100), stream_handler.listen()).await;
            stream_handler
        });

        let stream_handler = join_handle.await.expect("Task should succeed");
        for _ in 0..10 {
            let _ = receiver3.try_next().unwrap().unwrap();
        }

        // In this case the receiver is not closed, because we didn't send a fin.
        assert!(matches!(receiver3.try_next(), Err(_)));

        // Stream_id3 should still be there, because we didn't send a fin.
        let values = vec![1];
        assert!(stream_handler.stream_data.clone().into_keys().all(|item| values.contains(&item)));

        // But the buffer should be empty, as we've successfully drained it all.
        assert!(stream_handler.stream_data[&stream_id3].message_buffer.is_empty());
    }
}

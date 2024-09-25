use papyrus_protobuf::consensus::{ConsensusMessage, Proposal, StreamMessage};

use super::StreamHandler;

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn make_random_message(
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

    // check if two vectors are the same
    // ref: https://stackoverflow.com/a/58175659
    fn do_vecs_match<T: PartialEq>(a: &Vec<T>, b: &Vec<T>) -> bool {
        let matching = a.iter().zip(b.iter()).filter(|&(a, b)| a == b).count();
        matching == a.len() && matching == b.len()
    }

    fn setup_test() -> (
        StreamHandler<ConsensusMessage>,
        futures::channel::mpsc::Sender<StreamMessage<ConsensusMessage>>,
        futures::channel::mpsc::Receiver<ConsensusMessage>,
    ) {
        let (tx_input, rx_input) =
            futures::channel::mpsc::channel::<StreamMessage<ConsensusMessage>>(100);
        let (tx_output, rx_output) = futures::channel::mpsc::channel::<ConsensusMessage>(100);
        let handler = StreamHandler::new(tx_output, rx_input);
        (handler, tx_input, rx_output)
    }

    #[tokio::test]
    async fn test_stream_handler_in_order() {
        let (mut h, mut tx_input, mut rx_output) = setup_test();

        let stream_id = 127;
        for i in 0..10 {
            let message = make_random_message(stream_id, i, i == 9);
            tx_input.try_send(message).expect("Send should succeed");
        }

        let join_handle = tokio::spawn(async move {
            let _ = tokio::time::timeout(Duration::from_millis(100), h.listen()).await;
        });

        join_handle.await.expect("Task should succeed");

        for i in 0..10 {
            let _ = rx_output
                .try_next()
                .expect(&format!("Receive message {i} should succeed"))
                .expect(&format!("Receive message {i} should succeed"));
        }
    }

    #[tokio::test]
    async fn test_stream_handler_in_reverse() {
        let (mut h, mut tx_input, mut rx_output) = setup_test();

        let stream_id = 127;
        for i in 0..5 {
            let message = make_random_message(stream_id, 5 - i, i == 0);
            tx_input.try_send(message).expect("Send should succeed");
        }

        let join_handle = tokio::spawn(async move {
            let _ = tokio::time::timeout(Duration::from_millis(100), h.listen()).await;
            h
        });
        let mut h = join_handle.await.expect("Task should succeed");

        // check that the channel is empty (no messages were sent yet)
        assert!(rx_output.try_next().is_err());

        assert_eq!(h.stream_data.len(), 1);
        assert_eq!(h.stream_data[&stream_id].message_buffer.len(), 5);
        let range: Vec<u64> = (1..6).collect();
        let keys = h.stream_data[&stream_id].message_buffer.clone().into_keys().collect();
        assert!(do_vecs_match(&keys, &range));

        // now send the last message
        tx_input.try_send(make_random_message(stream_id, 0, false)).expect("Send should succeed");

        let join_handle = tokio::spawn(async move {
            let _ = tokio::time::timeout(Duration::from_millis(100), h.listen()).await;
            h
        });

        let h = join_handle.await.expect("Task should succeed");
        assert!(h.stream_data.is_empty());

        for i in 0..6 {
            let _ = rx_output
                .try_next()
                .expect(&format!("Receive message {i} should succeed"))
                .expect(&format!("Receive message {i} should succeed"));
        }
    }

    #[tokio::test]
    async fn test_stream_handler_multiple_streams() {
        let (mut h, mut tx_input, mut rx_output) = setup_test();

        let stream_id1 = 127; // send all messages in order (except the first one)
        let stream_id2 = 10; // send in reverse order (except the first one)
        let stream_id3 = 1; // send in two batches of 5 messages, without the first one, don't send fin

        for i in 1..10 {
            let message = make_random_message(stream_id1, i, i == 9);
            tx_input.try_send(message).expect("Send should succeed");
        }

        for i in 0..5 {
            let message = make_random_message(stream_id2, 5 - i, i == 0);
            tx_input.try_send(message).expect("Send should succeed");
        }

        for i in 5..10 {
            let message = make_random_message(stream_id3, i, false);
            tx_input.try_send(message).expect("Send should succeed");
        }
        for i in 1..5 {
            let message = make_random_message(stream_id3, i, false);
            tx_input.try_send(message).expect("Send should succeed");
        }

        let join_handle = tokio::spawn(async move {
            let _ = tokio::time::timeout(Duration::from_millis(100), h.listen()).await;
            h
        });
        let mut h = join_handle.await.expect("Task should succeed");

        let values = vec![1, 10, 127];
        assert!(h.stream_data.clone().into_keys().all(|item| values.contains(&item)));

        // We have all message from 1 to 9 buffered.
        assert!(do_vecs_match(
            &h.stream_data[&stream_id1].message_buffer.clone().into_keys().collect(),
            &(1..10).collect()
        ));

        // We have all message from 1 to 5 buffered.
        assert!(do_vecs_match(
            &h.stream_data[&stream_id2].message_buffer.clone().into_keys().collect(),
            &(1..6).collect()
        ));

        // We have all message from 1 to 5 buffered.
        assert!(do_vecs_match(
            &h.stream_data[&stream_id3].message_buffer.clone().into_keys().collect(),
            &(1..10).collect()
        ));

        // send the last message on stream_id1
        tx_input.try_send(make_random_message(stream_id1, 0, false)).expect("Send should succeed");
        let join_handle = tokio::spawn(async move {
            let _ = tokio::time::timeout(Duration::from_millis(100), h.listen()).await;
            h
        });

        let mut h = join_handle.await.expect("Task should succeed");

        // should be able to read all the messages for stream_id1
        for i in 0..10 {
            let _ = rx_output
                .try_next()
                .expect(&format!("Receive message {i} should succeed"))
                .expect(&format!("Receive message {i} should succeed"));
        }

        // stream_id1 should be gone
        let values = vec![1, 10];
        assert!(h.stream_data.clone().into_keys().all(|item| values.contains(&item)));

        // send the last message on stream_id2
        tx_input.try_send(make_random_message(stream_id2, 0, false)).expect("Send should succeed");
        let join_handle = tokio::spawn(async move {
            let _ = tokio::time::timeout(Duration::from_millis(100), h.listen()).await;
            h
        });

        let mut h = join_handle.await.expect("Task should succeed");

        // should be able to read all the messages for stream_id2
        for i in 0..6 {
            let _ = rx_output
                .try_next()
                .expect(&format!("Receive message {i} should succeed"))
                .expect(&format!("Receive message {i} should succeed"));
        }

        // stream_id2 should also be gone
        let values = vec![1];
        assert!(h.stream_data.clone().into_keys().all(|item| values.contains(&item)));

        // send the last message on stream_id3
        tx_input.try_send(make_random_message(stream_id3, 0, false)).expect("Send should succeed");

        let join_handle = tokio::spawn(async move {
            let _ = tokio::time::timeout(Duration::from_millis(100), h.listen()).await;
            h
        });

        let h = join_handle.await.expect("Task should succeed");
        for i in 0..10 {
            let _ = rx_output
                .try_next()
                .expect(&format!("Receive message {i} should succeed"))
                .expect(&format!("Receive message {i} should succeed"));
        }

        // stream_id3 should still be there, because we didn't send a fin
        let values = vec![1];
        assert!(h.stream_data.clone().into_keys().all(|item| values.contains(&item)));

        // but the buffer should be empty, as we've successfully drained it all
        assert!(h.stream_data[&stream_id3].message_buffer.is_empty());
    }
}

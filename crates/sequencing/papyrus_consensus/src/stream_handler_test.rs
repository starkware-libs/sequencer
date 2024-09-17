use papyrus_protobuf::consensus::{ConsensusMessage, Proposal, StreamMessage};

use super::{StreamHandler, StreamHandlerConfig};

#[cfg(test)]
mod tests {
    use super::*;

    fn make_random_message(
        stream_id: u64,
        chunk_id: u64,
        fin: bool,
    ) -> StreamMessage<ConsensusMessage> {
        StreamMessage {
            message: ConsensusMessage::Proposal(Proposal::default()),
            stream_id,
            chunk_id,
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
        futures::channel::mpsc::Receiver<StreamMessage<ConsensusMessage>>,
    ) {
        let (tx_input, rx_input) =
            futures::channel::mpsc::channel::<StreamMessage<ConsensusMessage>>(100);
        let (tx_output, rx_output) =
            futures::channel::mpsc::channel::<StreamMessage<ConsensusMessage>>(100);
        let config = StreamHandlerConfig::default();
        let handler = StreamHandler::new(config, tx_output, rx_input);
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
        tx_input.close_channel(); // this should signal the handler to break out of the loop

        let join_handle = tokio::spawn(async move {
            h.listen().await;
            h
        });

        let h = join_handle.await.expect("Task should succeed");
        assert!(h.message_buffers.is_empty());

        for i in 0..10 {
            let message = rx_output
                .try_next()
                .expect(&format!("Receive message {i} should succeed"))
                .expect(&format!("Receive message {i} should succeed"));
            assert_eq!(message.stream_id, stream_id);
            assert_eq!(message.chunk_id, i);
            if i == 9 {
                assert_eq!(message.fin, true);
            }
        }
    }

    #[tokio::test]
    async fn test_stream_handler_in_reverse() {
        let (mut h, mut tx_input, mut rx_output) = setup_test();
        h.config.timeout_millis = Some(100);

        let stream_id = 127;
        for i in 0..5 {
            let message = make_random_message(stream_id, 5 - i, i == 0);
            tx_input.try_send(message).expect("Send should succeed");
        }

        let join_handle = tokio::spawn(async move {
            h.listen().await;
            h
        });
        let mut h = join_handle.await.expect("Task should succeed");
        // println!("Handler.message_buffers: {:?}", h.message_buffers);
        assert_eq!(h.message_buffers.len(), 1);
        assert_eq!(h.message_buffers[&stream_id].len(), 5);
        let range: Vec<u64> = (1..6).collect();
        let keys = h.message_buffers[&stream_id].clone().into_keys().collect();
        assert!(do_vecs_match(&keys, &range));

        // now send the last message
        tx_input.try_send(make_random_message(stream_id, 0, false)).expect("Send should succeed");

        tx_input.close_channel(); // this should signal the handler to break out of the loop

        let join_handle = tokio::spawn(async move {
            h.listen().await;
            h
        });

        let h = join_handle.await.expect("Task should succeed");
        println!("Handler.message_buffers: {:?}", h.message_buffers);
        assert!(h.message_buffers.is_empty());

        for i in 0..6 {
            let message = rx_output
                .try_next()
                .expect(&format!("Receive message {i} should succeed"))
                .expect(&format!("Receive message {i} should succeed"));
            assert_eq!(message.stream_id, stream_id);
            assert_eq!(message.chunk_id, i);
            if i == 5 {
                assert_eq!(message.fin, true);
            }
        }
    }

    #[tokio::test]
    async fn test_stream_handler_multiple_streams() {
        let (mut h, mut tx_input, mut rx_output) = setup_test();
        h.config.timeout_millis = Some(100);

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
            h.listen().await;
            h
        });
        let mut h = join_handle.await.expect("Task should succeed");

        assert!(do_vecs_match(&h.message_buffers.clone().into_keys().collect(), &vec![1, 10, 127]));

        // the first case we have all message from 1 to 9 buffered into one contiguous sequence
        assert!(do_vecs_match(
            &h.message_buffers[&stream_id1].clone().into_keys().collect(),
            &(1..2).collect()
        ));

        // the second case we have all message from 1 to 5 buffered, each into its own vector (worse
        // case scenario)
        assert!(do_vecs_match(
            &h.message_buffers[&stream_id2].clone().into_keys().collect(),
            &(1..6).collect()
        ));

        // the third case we have two vectors, one with messages 1 to 4 and the other with messages
        // 5 to 9
        assert!(do_vecs_match(
            &h.message_buffers[&stream_id3].clone().into_keys().collect(),
            &vec![1, 5]
        ));

        // send the last message on stream_id1
        tx_input.try_send(make_random_message(stream_id1, 0, false)).expect("Send should succeed");
        let join_handle = tokio::spawn(async move {
            h.listen().await;
            h
        });

        let mut h = join_handle.await.expect("Task should succeed");

        // should be able to read all the messages for stream_id1
        for i in 0..10 {
            let message = rx_output
                .try_next()
                .expect(&format!("Receive message {i} should succeed"))
                .expect(&format!("Receive message {i} should succeed"));
            assert_eq!(message.stream_id, stream_id1);
            assert_eq!(message.chunk_id, i);
            if i == 9 {
                assert_eq!(message.fin, true);
            }
        }

        // stream_id1 should be gone
        println!(
            "Handler.message_buffers.keys= {:?}",
            h.message_buffers.keys().collect::<Vec<&u64>>()
        );
        assert!(do_vecs_match(&h.message_buffers.clone().into_keys().collect(), &vec![1, 10]));

        // the other two streams should be the same as before
        assert!(do_vecs_match(
            &h.message_buffers[&stream_id2].clone().into_keys().collect(),
            &(1..6).collect()
        ));
        assert!(do_vecs_match(
            &h.message_buffers[&stream_id3].clone().into_keys().collect(),
            &vec![1, 5]
        ));

        // send the last message on stream_id2
        tx_input.try_send(make_random_message(stream_id2, 0, false)).expect("Send should succeed");
        let join_handle = tokio::spawn(async move {
            h.listen().await;
            h
        });

        let mut h = join_handle.await.expect("Task should succeed");

        // should be able to read all the messages for stream_id2
        for i in 0..6 {
            let message = rx_output
                .try_next()
                .expect(&format!("Receive message {i} should succeed"))
                .expect(&format!("Receive message {i} should succeed"));
            assert_eq!(message.stream_id, stream_id2);
            assert_eq!(message.chunk_id, i);
            if i == 5 {
                assert_eq!(message.fin, true);
            }
        }

        // stream_id2 should also be gone
        assert!(do_vecs_match(&h.message_buffers.clone().into_keys().collect(), &vec![1]));

        // the last stream should be the same as before
        assert!(do_vecs_match(
            &h.message_buffers[&stream_id3].clone().into_keys().collect(),
            &vec![1, 5]
        ));

        // send the last message on stream_id3
        tx_input.try_send(make_random_message(stream_id3, 0, false)).expect("Send should succeed");
        tx_input.close_channel(); // this should signal the handler to break out of the loop

        let join_handle = tokio::spawn(async move {
            h.listen().await;
            h
        });

        let h = join_handle.await.expect("Task should succeed");
        for i in 0..10 {
            let message = rx_output
                .try_next()
                .expect(&format!("Receive message {i} should succeed"))
                .expect(&format!("Receive message {i} should succeed"));
            assert_eq!(message.stream_id, stream_id3);
            assert_eq!(message.chunk_id, i);
            assert_eq!(message.fin, false);
        }

        // stream_id3 should still be there, because we didn't send a fin
        assert!(do_vecs_match(&h.message_buffers.clone().into_keys().collect(), &vec![1]));

        // but the buffer should be empty, as we've successfully drained it all
        assert!(h.message_buffers[&stream_id3].is_empty());
    }

    #[tokio::test]
    #[should_panic]
    async fn test_stream_handler_duplicate_message_fails() {
        let (mut h, mut tx_input, _rx_output) = setup_test();
        tx_input.try_send(make_random_message(13, 42, false)).expect("Send should succeed");
        tx_input.try_send(make_random_message(13, 42, false)).expect("Send should succeed");
        tx_input.close_channel(); // this should signal the handler to break out of the loop

        // this should panic since we are sending the same message twice!
        let join_handle = tokio::spawn(async move {
            h.listen().await;
            h
        });

        h = join_handle.await.expect("Task should succeed");
        println!("Handler.message_buffers: {:?}", h.message_buffers);
    }

    #[tokio::test]
    #[should_panic]
    async fn test_stream_handler_after_fin_message_fails() {
        let (mut h, mut tx_input, _rx_output) = setup_test();
        tx_input.try_send(make_random_message(13, 42, true)).expect("Send should succeed");
        tx_input.try_send(make_random_message(13, 45, false)).expect("Send should succeed");
        tx_input.close_channel(); // this should signal the handler to break out of the loop

        // this should panic since the fin was received on chunk_id 42, but we are sending 45
        let join_handle = tokio::spawn(async move {
            h.listen().await;
            h
        });

        h = join_handle.await.expect("Task should succeed");
        println!("Handler.message_buffers: {:?}", h.message_buffers);
    }

    #[tokio::test]
    #[should_panic]
    async fn test_stream_handler_after_fin_message_reverse_fails() {
        let (mut h, mut tx_input, _rx_output) = setup_test();
        tx_input.try_send(make_random_message(13, 45, false)).expect("Send should succeed");
        tx_input.try_send(make_random_message(13, 42, true)).expect("Send should succeed");
        tx_input.close_channel(); // this should signal the handler to break out of the loop

        // this should panic since the fin was received on chunk_id 42, but we are sending 45
        let join_handle = tokio::spawn(async move {
            h.listen().await;
            h
        });

        h = join_handle.await.expect("Task should succeed");
        println!("Handler.message_buffers: {:?}", h.message_buffers);
    }

    #[tokio::test]
    #[should_panic]
    async fn test_stream_handler_max_buffer_fails() {
        let (mut h, mut tx_input, _rx_output) = setup_test();
        h.config.max_buffer_size = Some(10);
        // skip the first message, so the messages all get buffered
        for i in 0..11 {
            tx_input.try_send(make_random_message(13, i + 1, false)).expect("Send should succeed");
        }
        tx_input.close_channel(); // this should signal the handler to break out of the loop

        // this should panic since there are too many buffered messages
        let join_handle = tokio::spawn(async move {
            h.listen().await;
            h
        });

        h = join_handle.await.expect("Task should succeed");
        println!("Handler.message_buffers: {:?}", h.message_buffers);
    }

    #[tokio::test]
    #[should_panic]
    async fn test_stream_handler_max_streams_fails() {
        let (mut h, mut tx_input, _rx_output) = setup_test();
        h.config.max_num_streams = Some(10);
        // skip the first message, so the messages all get buffered
        for i in 0..11 {
            tx_input.try_send(make_random_message(i, 1, false)).expect("Send should succeed");
        }
        tx_input.close_channel(); // this should signal the handler to break out of the loop

        // this should panic since there are too many streams at the same time
        let join_handle = tokio::spawn(async move {
            h.listen().await;
            h
        });

        h = join_handle.await.expect("Task should succeed");
        println!("Handler.message_buffers: {:?}", h.message_buffers);
    }
}

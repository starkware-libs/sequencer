use std::time::SystemTime;
use std::vec;

use clap::Command;
use converters::{StressTestMessage, METADATA_SIZE};
use futures::StreamExt;
use libp2p::gossipsub::Topic;
use papyrus_config::loading::load_and_process_config;
use papyrus_network::network_manager::{BroadcastTopicClientTrait, NetworkManager};
use tokio::time::timeout;
use utils::{Record, TestConfig, BOOTSTRAP_CONFIG_FILE_PATH};

mod converters;
mod utils;

#[tokio::main]
async fn main() {
    let args = std::env::args().collect::<Vec<String>>();
    let default_path = BOOTSTRAP_CONFIG_FILE_PATH.to_string();
    let config_path = args.get(1).unwrap_or(&default_path);
    let file = std::fs::File::open(config_path).unwrap();
    let TestConfig { network_config, buffer_size, message_size, num_messages, output_path } =
        load_and_process_config(file, Command::new("Stress Test"), vec![]).unwrap();
    let mut network_manager = NetworkManager::new(network_config, None);
    let peer_id = network_manager.get_local_peer_id();
    let mut network_channels = network_manager
        .register_broadcast_topic::<StressTestMessage>(
            Topic::new("stress_test_topic".to_string()),
            buffer_size,
        )
        .unwrap();
    let mut output_vector = Vec::<Record>::new();
    tokio::select! {
        _ = network_manager.run() => {}
        _ = async {
            let mut i = 0;
            tokio::time::sleep(std::time::Duration::from_secs(20)).await;
            loop {
                let message = StressTestMessage::new(i, vec![0; message_size - METADATA_SIZE], peer_id.clone());
                network_channels.broadcast_topic_client.broadcast_message(message).await.unwrap();
                i += 1;
                if i == num_messages {
                    println!("Finished sending messages");
                    futures::future::pending::<()>().await;
                }
            }
        } => {}
        _ = async {
            let mut i = 0;
            loop {
                let maybe_response = timeout(
                    std::time::Duration::from_secs(120),
                    network_channels.broadcasted_messages_receiver.next(),
                ).await;
                match maybe_response {
                    Err(_) => {
                        println!("Timeout on message {}", i);
                        break;
                    }
                    Ok(None) => break,
                    Ok(Some((received_message, _report_callback))) => {
                        let received_message = received_message.unwrap();
                        output_vector.push(Record {
                            peer_id: received_message.peer_id,
                            id: received_message.id,
                            start_time: received_message.time,
                            end_time: SystemTime::now(),
                            duration: SystemTime::now()
                                .duration_since(received_message.time)
                                .unwrap()
                                .as_micros(),
                        });
                        i += 1;
                        if i == num_messages * 4 {
                            tokio::time::sleep(std::time::Duration::from_secs(20)).await;
                            break;
                        }
                    }
                }
            }
        } => {
            println!("Finished receiving messages");
            let mut wtr = csv::Writer::from_path(output_path).unwrap();
            for record in output_vector {
                wtr.serialize(record).unwrap();
            }
        }
    }
}

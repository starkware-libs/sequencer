use apollo_infra_utils::template::Template;
use serde_json::Value;
const REPLACER_FORMAT: &str = "$$$_{}_$$$";

use libp2p::{identity, Multiaddr, PeerId, Stream, StreamProtocol};

const MULTIADDR_KEYS: [&str; 4] = [
    "consensus_manager_config.network_config.bootstrap_peer_multiaddr",
    "mempool_p2p_config.network_config.bootstrap_peer_multiaddr",
    "consensus_manager_config.network_config.advertised_multiaddr",
    "mempool_p2p_config.network_config.advertised_multiaddr",
];

pub(crate) fn insert_replacer_annotations<F>(mut json: Value, pred: F) -> Value
where
    F: Fn(&str, &Value) -> bool,
{
    let map = json.as_object_mut().expect("Should be a JSON object");

    // Collect keys to avoid mutable borrow issues while iterating.
    let keys: Vec<String> = map.keys().cloned().collect();
    for key in keys {
        let should_replace = {
            // Evaluate predicate on current value
            let value = map.get(&key).expect("Key must exist");
            pred(&key, value)
        };

        if should_replace {
            if MULTIADDR_KEYS.contains(&key.as_str()) {
                let multiaddrs = extract_multiaddrs(map, &key);
                if let Some(multiaddrs) = multiaddrs {
                    map.insert(
                        key.clone(),
                        Value::Array(
                            multiaddrs
                                .into_iter()
                                .map(|addr| Value::String(addr.to_string()))
                                .collect(),
                        ),
                    );
                }
            } else {
                map.insert(key.clone(), Value::String(format_key(key.clone())));
            }
        }
    }

    json
}

fn format_key(key: String) -> String {
    Template::new(REPLACER_FORMAT).format(&[&key]).to_uppercase().replace('.', "-").replace('#', "")
}

fn parse_multiaddrs(value: &str) -> Vec<Multiaddr> {
    value
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse::<Multiaddr>().ok()) // skip invalid addresses
        .collect()
}

pub fn extract_multiaddrs(
    json: &serde_json::Map<std::string::String, serde_json::Value>,
    key: &str,
) -> Option<Vec<Multiaddr>> {
    json.get(key)?.as_str().map(parse_multiaddrs)
}

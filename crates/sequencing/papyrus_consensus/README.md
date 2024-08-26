# papyrus-consensus

This crate provides an implementation of consensus for a Starknet node.

### Disclaimer
This crate is still under development; expect many breaking changes.

## How to run
1. You must turn consensus on and provide a validator ID by passing: `--consensus.#is_none false --consensus.validator_id 0x<UNIQUE>`
    1. By default the nodes expect 4 validators, with IDs: 0-3.
2. Start by running any nodes which are validators for `consensus.start_height` (default 0) to avoid them missing the proposal.
3. You can test the consensus under simulated network conditions, by passing: `--consensus.test.#is_none false`
* The node's configuration can be customized using the relevant consensus flags. For more details, see the consensus flags (prefixed with consensus) by running the following command:
    * `cargo run --package papyrus_node --bin papyrus_node -- --help`

#### Bootstrap Node
This must be run first:
```
cargo run --package papyrus_node --bin papyrus_node -- --base_layer.node_url <ETH_NODE_URL> --network.#is_none false --consensus.#is_none false --consensus.validator_id 0x1 --storage.db_config.path_prefix <UNIQUE>
```
- This will log `local_peer_id` which is used by other nodes. (Alternatively pass `network.secret_key` to have a fixed peer id).

#### Other Nodes
Run each of the other nodes separately, using different `consensus.validator_id` {`0x2`, `0x3`, `0x0`}:

```
cargo run --package papyrus_node --bin papyrus_node -- --base_layer.node_url <ETH_NODE_URL> --network.#is_none false --consensus.#is_none false --consensus.validator_id 0x<UNIQUE> --network.tcp_port <UNIQUE> --network.bootstrap_peer_multiaddr.#is_none false --rpc.server_address 127.0.0.1:<UNIQUE> --monitoring_gateway.server_address 127.0.0.1:<UNIQUE> --storage.db_config.path_prefix <UNIQUE>  --network.bootstrap_peer_multiaddr /ip4/127.0.0.1/tcp/10000/p2p/<BOOT_NODE_PEER_ID> 
```
- Node 0 is the proposer and should be run last.

UNIQUE - a value unique among all nodes running locally.

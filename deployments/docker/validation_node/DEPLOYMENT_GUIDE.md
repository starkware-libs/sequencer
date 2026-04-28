# Validation-Only Node Deployment Guide

This guide explains how to deploy a Starknet validation-only node using Docker, with the Signature Manager running in a separate container for security isolation.

## Architecture

```
                    +-----------------------+
                    |   Docker Network      |
                    |  (validation-network) |
                    |                       |
  +---------------------+     HTTP      +---------------------+
  |  validation-node     |  port 9090   |  signature-manager   |
  |                      |<------------>|                      |
  |  Components:         |   (internal) |  Components:         |
  |  - Consensus Manager |              |  - Signature Manager |
  |  - Batcher           |              |  - Config Manager    |
  |  - Class Manager     |              |  - Monitoring        |
  |  - Committer         |              |                      |
  |  - State Sync        |              +---------------------+
  |  - Sierra Compiler   |               :8083 -> monitoring
  |  - L1 Events         |
  |  - L1 Gas Price      |
  |  - Proof Manager     |
  |  - Config Manager    |
  |  - Monitoring        |
  +---------------------+
   :8082 -> monitoring
```

**Why a separate Signature Manager?** The Signature Manager holds the validator's signing keys and performs cryptographic operations (signing peer identification and precommit votes). Running it in an isolated container limits the attack surface: even if the main node process is compromised, the signing keys remain in a separate process boundary.

Both containers run the same `apollo_node` binary. The configuration files determine which components are active in each container.

## Prerequisites

- Docker Engine 24.0+ and Docker Compose v2
- The `apollo_node` Docker image (see [Building the Docker Image](#building-the-docker-image))
- An Ethereum L1 node endpoint (for base layer interaction)
- Network connectivity to the Starknet P2P network (for consensus and state sync)

## Quick Start

### 1. Clone and navigate to the deployment directory

```bash
cd deployments/docker/validation_node
```

### 2. Create your environment file

```bash
cp .env.example .env
```

Edit `.env` and set:
- `IMAGE_TAG` - the Docker image version to use
- `IMAGE_REGISTRY` - container registry URL (default: `ghcr.io/starkware-libs/sequencer`)
- `DATA_DIR` - host path for persistent data (default: `./data`)

### 3. Configure the validation node

Edit `config/validation_node.json` and replace all `<PLACEHOLDER>` values with your actual configuration. See [Configuration Reference](#configuration-reference) for details on each placeholder.

### 4. Start the deployment

```bash
docker compose up -d
```

### 5. Verify the deployment

```bash
# Check both containers are running
docker compose ps

# View logs
docker compose logs -f

# Check monitoring endpoints
curl http://localhost:8082/monitoring/alive    # validation node
curl http://localhost:8083/monitoring/alive    # signature manager
```

## Configuration Reference

### Placeholders in `config/validation_node.json`

All values marked with `<PLACEHOLDER>` must be replaced before starting the node.

#### Network Identity

| Placeholder | Description | Example |
|---|---|---|
| `<CHAIN_ID>` | The Starknet chain identifier | `SN_MAIN` or `SN_SEPOLIA` |
| `<VALIDATOR_ID>` | Your validator's public key | `0x1234...abcd` |

#### Token Addresses

| Placeholder | Description | Example (Mainnet) |
|---|---|---|
| `<ETH_FEE_TOKEN_ADDRESS>` | ETH fee token contract address | `0x049d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7` |
| `<STRK_FEE_TOKEN_ADDRESS>` | STRK fee token contract address | `0x04718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d` |

#### External Endpoints

| Placeholder | Description | Example |
|---|---|---|
| `<L1_ENDPOINT_URL>` | Ethereum L1 node endpoint (HTTP or WebSocket) | `https://eth-mainnet.example.com` |
| `<STARKNET_CONTRACT_ADDRESS>` | Starknet core contract address on L1 | `0xc662c410C0ECf747543f5bA90660f6ABeBD9C8c4` |
| `<STARKNET_URL>` | Starknet RPC endpoint for state sync | `https://starknet-rpc.example.com` |
| `<RECORDER_URL>` | Block recorder service URL | `https://recorder.example.com` |
| `<ETH_TO_STRK_ORACLE_URL>` | ETH/STRK exchange rate oracle endpoint | `https://oracle.example.com/eth_to_strk` |

#### P2P Networking

| Placeholder | Description | Example |
|---|---|---|
| `<CONSENSUS_P2P_PORT>` | Port for consensus P2P communication | `10000` |
| `<CONSENSUS_ADVERTISED_MULTIADDR>` | Externally reachable address for consensus P2P | `/ip4/203.0.113.1/tcp/10000` |
| `<CONSENSUS_BOOTSTRAP_PEER_MULTIADDR>` | Bootstrap peer for consensus network | `/ip4/198.51.100.1/tcp/10000/p2p/12D3Koo...` |
| `<STATE_SYNC_ADVERTISED_MULTIADDR>` | Externally reachable address for state sync P2P | `/ip4/203.0.113.1/tcp/53140` |
| `<STATE_SYNC_BOOTSTRAP_PEER_MULTIADDR>` | Bootstrap peer for state sync network | `/ip4/198.51.100.1/tcp/53140/p2p/12D3Koo...` |

#### Consensus

| Placeholder | Description | Example |
|---|---|---|
| `<DEFAULT_COMMITTEE>` | Default validator committee configuration | (network-specific JSON) |

### Placeholders in `config/signature_manager.json`

The signature manager config has no installation-specific placeholders. It runs the Signature Manager component with default settings, serving HTTP on port 9090.

**Note:** The validator's signing key configuration is handled by the `apollo_node` binary at startup (via environment variables or key file paths, depending on your key management setup). Refer to the Starknet documentation for key configuration details.

## Container Details

### Validation Node (`validation-node`)

| Property | Value |
|---|---|
| Image | `sequencer` (runs the `apollo_node` binary) |
| Config | `/config/validation_node.json` |
| Data Volume | `/data` (batcher, consensus, state_sync, committer, class_manager, proofs) |
| Host Ports | 8082 (monitoring), consensus P2P, 53140 (state sync P2P) |
| Mode | `validation_only: true` |

**Enabled components:** Batcher, Class Manager, Committer, Config Manager, Consensus Manager, L1 Events Provider, L1 Events Scraper, L1 Gas Price Provider, L1 Gas Price Scraper, Monitoring Endpoint, Proof Manager, Sierra Compiler, State Sync.

**Disabled components:** Gateway, HTTP Server, Mempool, Mempool P2P (required for validation-only mode).

**Remote components:** Signature Manager (connects to `signature-manager:9090` via HTTP).

### Signature Manager (`signature-manager`)

| Property | Value |
|---|---|
| Image | `sequencer` (same image, runs the `apollo_node` binary) |
| Config | `/config/signature_manager.json` |
| Data Volume | None (stateless) |
| Host Port | 8083 (monitoring, remapped from internal 8082) |
| Internal Port | 9090 (Signature Manager HTTP server, not exposed to host) |

**Enabled components:** Config Manager, Monitoring Endpoint, Signature Manager.

**All other components:** Disabled.

### Inter-Container Communication

The validation node connects to the signature manager over HTTP:
- Protocol: HTTP with JSON-serialized request/response
- Endpoint: `http://signature-manager:9090`
- Resolved via Docker DNS on the `validation-network` bridge network
- Operations: `SignIdentification` (peer identity signing) and `SignPrecommitVote` (consensus vote signing)
- Connection settings: 500ms timeout, 10 idle connections, up to 150 retries with exponential backoff

## Network Configuration

### Ports That Must Be Externally Reachable

For the validation node to participate in consensus, these ports must be accessible from the P2P network:

| Port | Protocol | Purpose |
|---|---|---|
| Consensus P2P port (configured via `<CONSENSUS_P2P_PORT>`) | TCP | Consensus protocol communication |
| 53140 (default) | TCP | State sync P2P communication |

### Host-Mapped Ports

| Host Port | Container | Internal Port | Purpose |
|---|---|---|---|
| 8082 | validation-node | 8082 | Monitoring endpoint |
| `CONSENSUS_P2P_PORT` (default 10000) | validation-node | `CONSENSUS_P2P_PORT` | Consensus P2P |
| 53140 | validation-node | 53140 | State sync P2P |
| 8083 | signature-manager | 8082 | Monitoring endpoint |

### Internal-Only Ports

| Port | Container | Purpose |
|---|---|---|
| 9090 | signature-manager | Signature Manager HTTP server (Docker network only) |

## Storage

All persistent data is stored under the configured `DATA_DIR` (default: `./data`).

### Validation Node Data Structure

```
${DATA_DIR}/validation_node/
  batcher/          # Block building data
  consensus/        # Consensus state and votes
  state_sync/       # Synchronized state data
  committer/        # State commitment (RocksDB)
  class_manager/    # Contract class storage
    class_hash_storage/
    classes/
  proofs/           # Proof data cache
```

All storage uses `StateOnly` scope (not `FullArchive`), which stores only the state necessary for validation. This significantly reduces disk usage compared to a full node.

### Signature Manager

The signature manager container is stateless and does not require persistent storage.

### Backup Considerations

- Stop the node before backing up: `docker compose down`
- Back up the entire `${DATA_DIR}/validation_node/` directory
- The `consensus/` subdirectory contains the last voted height; losing it may cause the node to re-vote on a height, which is safe but not ideal

## Operations

### Starting

```bash
docker compose up -d
```

The signature manager starts first, then the validation node (which depends on it being available).

### Stopping

```bash
docker compose down
```

### Viewing Logs

```bash
# All services
docker compose logs -f

# Specific service
docker compose logs -f validation-node
docker compose logs -f signature-manager
```

### Checking Health

```bash
# Monitoring endpoints
curl http://localhost:8082/monitoring/alive    # validation node
curl http://localhost:8083/monitoring/alive    # signature manager
```

### Updating

```bash
# Pull the new image version
docker compose pull

# Restart with the new image
docker compose up -d
```

Or update the `IMAGE_TAG` in `.env` and run:

```bash
docker compose up -d
```

## Security Considerations

1. **Key isolation**: The Signature Manager runs in a separate container, limiting the blast radius if the main node is compromised. Only the signature manager has access to the validator's signing keys.

2. **Network isolation**: The signature manager's HTTP port (9090) is only exposed on the Docker bridge network (`expose`), not mapped to the host. It is not reachable from outside the Docker network.

3. **Read-only config**: Config files are mounted read-only (`:ro`) into the containers.

4. **Minimal surface**: The signature manager runs only 3 components (Config Manager, Signature Manager, Monitoring). All other components are disabled, minimizing the code paths exposed.

5. **File permissions**: Ensure config files are readable only by the container user (UID 1001). On the host:
   ```bash
   chmod 640 config/validation_node.json config/signature_manager.json
   ```

6. **Data directory**: Ensure the data directory has appropriate permissions:
   ```bash
   mkdir -p data/validation_node
   chown -R 1001:1001 data/validation_node
   ```

## Troubleshooting

### Signature Manager unreachable

**Symptom:** Validation node logs show connection errors to `signature-manager:9090`.

**Checks:**
1. Verify both containers are on the same network: `docker network inspect validation_node_validation-network`
2. Verify the signature manager is running: `docker compose ps signature-manager`
3. Test DNS resolution from the validation node: `docker exec validation-node getent hosts signature-manager`
4. Check signature manager logs: `docker compose logs signature-manager`

### Node not syncing

**Symptom:** No new blocks being processed.

**Checks:**
1. Verify `<STARKNET_URL>` is reachable from the container
2. Verify `<L1_ENDPOINT_URL>` is reachable from the container
3. Check state sync P2P bootstrap peer connectivity
4. Verify `<CHAIN_ID>` matches the target network

### Consensus not participating

**Symptom:** Node is syncing but not voting on blocks.

**Checks:**
1. Verify `<VALIDATOR_ID>` is correct and registered as a validator
2. Verify `<CONSENSUS_BOOTSTRAP_PEER_MULTIADDR>` is correct
3. Verify the consensus P2P port is externally reachable
4. Check that the signature manager is responding (see above)
5. Verify `<DEFAULT_COMMITTEE>` is configured correctly

### Permission errors on data directory

**Symptom:** Container fails to start with permission denied errors.

**Fix:**
```bash
mkdir -p data/validation_node
chown -R 1001:1001 data/validation_node
```

The container runs as UID 1001 (`sequencer` user).

## Building the Docker Image

The default configuration pulls the image from `ghcr.io/starkware-libs/sequencer`. If you need to build from source instead:

```bash
# From the repository root
docker build \
  -f deployments/images/sequencer/Dockerfile \
  -t ghcr.io/starkware-libs/sequencer/sequencer:latest \
  .
```

This tags the image to match the default compose reference. If using a custom `IMAGE_REGISTRY` or `IMAGE_TAG` in `.env`, adjust the `-t` flag accordingly.

This requires the `dockerfile-x` Docker build extension. The build uses multi-stage builds with `cargo-chef` for efficient dependency caching.

**Build arguments:**
- `BUILD_MODE` - `release` (default) or `debug`

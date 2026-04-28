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
- Python 3 (used by `setup.sh` for config generation)
- The `apollo_node` Docker image (see [Building the Docker Image](#building-the-docker-image))
- An Ethereum L1 node endpoint (for base layer interaction)
- An ETH-to-STRK oracle endpoint
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
- `CONSENSUS_P2P_PORT` - must match the port you choose in the next step (default: `53080`)

### 3. Generate the node configuration

```bash
./setup.sh
```

The script interactively guides you through selecting the target network and providing your validator-specific values, then generates `config/validation_node.json`. See [Configuration via setup.sh](#configuration-via-setupsh) for details.

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

## Configuration via setup.sh

`setup.sh` generates `config/validation_node.json` from `config/validation_node.json.template`. It separates configuration into environment-specific values (loaded automatically from `environments/`) and validator-specific values (prompted interactively).

### Script flow

1. **Existing config check** — If `config/validation_node.json` already exists, you can keep it and skip straight to the Docker start prompt.
2. **Last choices** — If you have run `setup.sh` before, your previous inputs are shown and you can reuse all of them or review them setting-by-setting.
3. **Environment selection** — Choose the target network:
   - `1) production` — Mainnet (`SN_MAIN`)
   - `2) test` — Sepolia testnet (`SN_SEPOLIA`)
   - `3) integration` — Integration testnet (`SN_INTEGRATION_SEPOLIA`)
4. **Validator-specific prompts** — Collected interactively (see table below).
5. **Config generation** — Writes `config/validation_node.json` (mode `640`).
6. **Choices saved** — Inputs are saved to `config/.last_choices.json` (mode `600`, gitignored) for future runs.
7. **Docker prompt** — Optionally starts `docker compose up -d` immediately.

### Validator-specific values (prompted by setup.sh)

| Field | Description | Format |
|---|---|---|
| Validator ID | Your validator's public key | `0x1234...abcd` |
| L1 endpoint URLs | Ethereum RPC endpoints (one per line, multiple for redundancy) | `https://eth-mainnet.example.com` |
| ETH-to-STRK oracle URL+headers | Oracle endpoint(s) with optional auth headers (one per line) | `https://api.example.com/price,Authorization^Bearer token` |
| Consensus advertised multiaddr | Your externally reachable P2P address(es) (one per line) | `/ip4/203.0.113.1/tcp/53080` or `/dns/mynode.example.com/tcp/53080` |
| Consensus P2P port | Port for consensus P2P (**optional**, default: `53080`) | `53080` |

**Note on oracle format:** Each entry is `<url>,<header_key>^<header_value>`. The `timestamp` query parameter is appended automatically at query time. The comma and caret are literal delimiters, not separators between entries.

**Important:** The `CONSENSUS_P2P_PORT` in your `.env` file must match the port you enter here, as Docker uses it for the port mapping.

### Environment-specific values (loaded automatically)

These values are read from `environments/<env>.json` and require no manual input:

| Field | Description |
|---|---|
| `chain_id` | Network identifier (`SN_MAIN`, `SN_SEPOLIA`, `SN_INTEGRATION_SEPOLIA`) |
| `eth_fee_token_address` | ETH fee token contract address |
| `strk_fee_token_address` | STRK fee token contract address |
| `starknet_url` | Starknet feeder gateway URL for state sync |
| `starknet_contract_address` | Starknet core contract address on L1 |
| `bpo1_start_block_number` | BPO1 upgrade start block |
| `bpo2_start_block_number` | BPO2 upgrade start block |
| `fusaka_no_bpo_start_block_number` | Fusaka upgrade start block |
| `consensus_bootstrap_peer_multiaddr` | Bootstrap peers for the consensus P2P network |
| `default_committee` | Validator committee configuration |

### Signature manager config

`config/signature_manager.json` has no installation-specific values and requires no changes. It configures the Signature Manager component to serve HTTP on port 9090.

**Note:** The validator's signing key is configured via environment variables or key file paths at runtime, depending on your key management setup. Refer to the Starknet documentation for key configuration details.

## Container Details

### Validation Node (`validation-node`)

| Property | Value |
|---|---|
| Image | `sequencer` (runs the `apollo_node` binary) |
| Config | `/config/validation_node.json` |
| Data Volume | `/data` (batcher, consensus, state_sync, committer, class_manager, proofs) |
| Host Ports | 8082 (monitoring), consensus P2P port, 53140 (state sync P2P) |
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
| Consensus P2P port (configured in `setup.sh`, default `53080`) | TCP | Consensus protocol communication |
| 53140 (default) | TCP | State sync P2P communication |

### Host-Mapped Ports

| Host Port | Container | Internal Port | Purpose |
|---|---|---|---|
| 8082 | validation-node | 8082 | Monitoring endpoint |
| `CONSENSUS_P2P_PORT` (default 53080) | validation-node | `CONSENSUS_P2P_PORT` | Consensus P2P |
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

5. **Config file permissions**: `setup.sh` generates `config/validation_node.json` with mode `640` and `config/.last_choices.json` (which contains your secrets) with mode `600`. Verify:
   ```bash
   ls -l config/validation_node.json config/.last_choices.json
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
1. Verify the L1 endpoint URL is reachable from the container
2. Verify the Starknet feeder URL is reachable from the container
3. Check state sync P2P bootstrap peer connectivity
4. Verify `chain_id` in the generated config matches the target network

### Consensus not participating

**Symptom:** Node is syncing but not voting on blocks.

**Checks:**
1. Verify the validator ID is correct and registered as a validator
2. Verify the consensus bootstrap peer multiaddr is correct
3. Verify the consensus P2P port is externally reachable
4. Verify `CONSENSUS_P2P_PORT` in `.env` matches the port in `config/validation_node.json`
5. Check that the signature manager is responding (see above)

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

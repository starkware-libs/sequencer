# Unlimited Congestion Controller

## Overview

The `UnlimitedCongestionConfig` provides a congestion controller that behaves like UDP with **NO congestion control**. It ignores all packet loss and congestion signals, maintaining a constant large congestion window.

## Warning

⚠️ **USE WITH CAUTION**: This congestion controller:
- Ignores ALL packet loss signals
- Never reduces its sending rate
- Can be unfair to other network traffic
- May cause or worsen network congestion
- Should ONLY be used in controlled environments

## When to Use

This controller is appropriate for:

1. **Controlled Networks**: Private networks where you control all endpoints and understand the capacity
2. **High-Throughput Applications**: Applications prioritizing maximum throughput over fairness
3. **Testing and Benchmarking**: Performance testing scenarios
4. **Known High-BDP Networks**: Networks with known high bandwidth-delay product where standard congestion control is overly conservative

## Configuration

### Basic Usage

```rust
use apollo_quic_datagrams::{Config, CongestionController};
use libp2p::identity::Keypair;

let keypair = Keypair::generate_ed25519();

let quic_config = Config::new(&keypair)
    .with_congestion_controller(CongestionController::Unlimited {
        window: 1 << 30,  // 1GB congestion window
    });
```

### With Transport Configuration

```rust
use apollo_quic_datagrams::{Config, CongestionController};

let quic_config = Config::new(&keypair)
    .with_max_stream_data(10_000_000)           // 10MB per stream
    .with_max_connection_data(100_000_000)      // 100MB total
    .with_send_window(100_000_000)              // 100MB send window
    .with_congestion_controller(CongestionController::Unlimited {
        window: 1 << 30,                        // 1GB congestion window
    });
```

## How It Works

The unlimited congestion controller:

1. **Starts** with a very large congestion window (configurable, default 1GB)
2. **Never reduces** the window on packet loss
3. **Ignores** all congestion events
4. **Maintains** constant sending rate regardless of network conditions

### Comparison with Other Controllers

| Controller | Reacts to Loss | Fair to Others | Use Case |
|------------|----------------|----------------|----------|
| **NewReno** | ✅ Yes | ✅ Yes | General purpose, conservative |
| **BBR** | ⚠️ Partially | ⚠️ Somewhat | High-throughput, BDP-aware |
| **Unlimited** | ❌ No | ❌ No | Controlled environments only |

## Implementation Details

```rust
// The controller implements the quinn::congestion::Controller trait
// but ignores all signals:

fn on_ack(&mut self, ...) {
    // Does nothing - window stays at maximum
}

fn on_congestion_event(&mut self, ...) {
    // Ignores packet loss - window stays at maximum
}
```

## Performance Considerations

### Advantages
- **Maximum throughput** in favorable conditions
- **No backoff delays** from loss detection
- **Consistent performance** in high-latency networks

### Disadvantages
- **Can overwhelm networks** causing cascading failures
- **Unfair to other traffic** on shared networks
- **May worsen congestion** in already-congested networks
- **No adaptation** to changing network conditions

## Monitoring

When using the unlimited congestion controller, monitor:

1. **Packet Loss Rates**: Via QUIC connection statistics logging
2. **Network Saturation**: Via system network metrics
3. **Other Applications**: Ensure they're not being starved of bandwidth

See [LOGGING.md](LOGGING.md) for details on QUIC statistics logging.

## Example: High-Throughput P2P

```rust
use apollo_quic_datagrams::{Config, CongestionController};
use apollo_quic_datagrams::tokio::Transport;

// Configuration for maximum throughput in a controlled network
let quic_config = Config::new(&keypair)
    .with_max_stream_data(50_000_000)      // 50MB per stream
    .with_max_connection_data(500_000_000)  // 500MB total
    .with_send_window(500_000_000)          // 500MB send window
    .with_congestion_controller(CongestionController::Unlimited {
        window: 1 << 31,                    // 2GB congestion window
    });

let transport = Transport::new(quic_config);
```

## Testing

The unlimited congestion controller includes unit tests that verify:
- ✅ Window never reduces on congestion events
- ✅ Custom window sizes work correctly
- ✅ Multiple severe loss events don't affect window

Run tests with:
```bash
cargo test -p apollo_quic_datagrams --lib unlimited_congestion
```

## See Also

- [LOGGING.md](LOGGING.md) - QUIC connection statistics logging
- [libp2p QUIC documentation](https://docs.rs/libp2p-quic/)
- [QUINN documentation](https://docs.rs/quinn/)
- [QUIC specification](https://www.rfc-editor.org/rfc/rfc9000.html)


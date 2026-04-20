//! L1 gas price feed for the Starknet sequencer.
//!
//! # Price Selection and Fallback Strategy
//!
//! Each block proposal needs two independent inputs:
//! - **L1 gas prices (WEI)** — mean `base_fee_per_gas` and `blob_fee` over the last N Ethereum
//!   blocks, maintained by [`l1_gas_price_provider`].
//! - **ETH→STRK rate** — current market rate from one of several oracle endpoints, queried by
//!   [`eth_to_strk_oracle`]. Multiple URLs are tried in round-robin; if the in-flight query for
//!   the current time bucket hasn't resolved yet, the previous bucket's cached rate is used.
//!
//! When combining these in `apollo_consensus_orchestrator`, the fallback chain is:
//!
//! 1. **Live data** — Both sources succeed → transform prices (tip, clamp, multiplier) and convert
//!    to FRI using the live rate. Normal path.
//! 2. **Previous block** — Either source fails → reuse WEI and FRI prices from the last accepted
//!    proposal, back-calculating the ETH→STRK rate from them for consistency.
//! 3. **Minimal config** — No previous block available (e.g. startup) → use configured
//!    `min_l1_gas_price_wei` / `min_l1_data_gas_price_wei` and `DEFAULT_ETH_TO_FRI_RATE`
//!    (10²¹ FRI/ETH ≈ 1 ETH = 1000 STRK).
//!
//! **Why not halt on failure?** A transient oracle or provider outage should not stall block
//! production. Stale prices self-correct once the feed recovers; a halted chain requires manual
//! intervention and breaks liveness for all users.
//!
//! **Why keep fallback prices low?** When the true L1 cost is unknown, undercharging slightly is
//! better than overcharging. Inflated prices cause transactions to fail and make the network
//! unusable precisely when it is already degraded. The configured minimums and the default
//! ETH→STRK rate are safe floors the operator has verified are always economically viable.

pub mod communication;
pub mod eth_to_strk_oracle;
pub mod l1_gas_price_provider;
pub mod l1_gas_price_scraper;
pub mod metrics;

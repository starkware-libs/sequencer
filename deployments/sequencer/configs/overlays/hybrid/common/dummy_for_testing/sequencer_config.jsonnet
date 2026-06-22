// IN-PLACE nested-overrides for the hybrid `dummy_for_testing` overlay layer.
//
// Self-contained: holds only this layer's own `config.sequencerConfig` deltas, aggregated from
// `common.yaml` and `services/*.yaml` and converted to nested, `#is_none`-folded form. Layered LAST
// (deep-merged over `common` then `sepolia-integration`): it supplies the per-pod instance values —
// the P2P multiaddrs as dummy `None` and the `validator_id`. No k8s scaffolding, no `components.*`.
{
  validator_id: '0x64',
  consensus_manager_config: {
    network_config: {
      // `#is_none: true` -> dummy `None` multiaddrs fold to null.
      advertised_multiaddr: null,
      bootstrap_peer_multiaddr: null,
    },
  },
  mempool_p2p_config: {
    network_config: {
      advertised_multiaddr: null,
      bootstrap_peer_multiaddr: null,
    },
  },
}

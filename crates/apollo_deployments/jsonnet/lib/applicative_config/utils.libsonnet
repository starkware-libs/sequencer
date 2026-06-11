{
  advertisedMultiaddr(bootstrap_peer_multiaddr, node_index, nodes_at_same_cluster)::
    if !nodes_at_same_cluster && bootstrap_peer_multiaddr != null
    then std.split(bootstrap_peer_multiaddr, ',')[node_index]
    else null,
}

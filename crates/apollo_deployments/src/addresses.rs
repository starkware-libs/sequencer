use std::str::FromStr;

use libp2p::multiaddr::Protocol;
use libp2p::{Multiaddr, PeerId};

use crate::deployment::build_service_namespace_domain_address;
use crate::service::NodeService;

// The following peer ids are derived using `get_peer_id_from_secret_key` binary, where the secret
// key of node with index `id` is format!("
// 0x01010101010101010101010101010101010101010101010101010101010101{:02x}", id + 1)
//
// ```
// for i in `printf '0x01010101010101010101010101010101010101010101010101010101010101%02X\n'
// {1..40}`; do cargo run --bin get_peer_id_from_secret_key $i ; done 2>/dev/null | awk '/Peer/
// {printf("\t\"%s\",\n", $NF)}'
// ```

pub(crate) const PEER_IDS: [&str; 40] = [
    "12D3KooWK99VoVxNE7XzyBwXEzW7xhK7Gpv85r9F3V3fyKSUKPH5",
    "12D3KooWCPzcTZ4ymgyveYaFfZ4bfWsBEh2KxuxM3Rmy7MunqHwe",
    "12D3KooWT3eoCYeMPrSNnF1eQHimWFDiqPkna7FUD6XKBw8oPiMp",
    "12D3KooWFdTjV6DXVJfQFisTXadCsqGzCbEnJJWzc6mXSPwy9g54",
    "12D3KooWJMukrrip9sUyto28eiofqxyXiw9sfTJuZeQfHUujWPX8",
    "12D3KooWMqkzSDGNQg9WDDPdu7nQgAPpqTY3YqZ2XUYqJzmUhmVu",
    "12D3KooWCyVfryCPdY9XGpLDD8z8BWYzgfSPV14jKmurukP6j1dn",
    "12D3KooWD72UEaJmGGJqpumd4XqFGz4CX6vYnLhZCtX98kExPCGC",
    "12D3KooWHx58SJV5WH782PKhMyw7ejqAEokzfm49yuUd7FYuWR57",
    "12D3KooWBo4q9Kg95Cox5ECweamv26sxoV6b6pUhVJXEpcSv6Dq1",
    "12D3KooWHTbjQtxM3nTF85uadLTbDxX9DXjZzzwdtAogdqxGr9t5",
    "12D3KooWJ2JJbbMRt1YrERoMPfU4hwTASZkpVqbfGiyu4d3MGGhs",
    "12D3KooWLDHhUjWxXrye37UN23QMEiLEyDt9N6ZtSAsEs2guckW9",
    "12D3KooWKZu9RjwQfiRu6VxeCUqZr6fah8BeLjKErDEw1bVDL8X5",
    "12D3KooWNEQduhMrckKco8bvmPXZvXuGo5RQNdhxL7nr6rwydJ4V",
    "12D3KooWEGF9d7XwCFL9mu6co9z7tiDA4whzVHWuZqHAjXRaYTeS",
    "12D3KooWAkRS6QAJwayUBT55CkvWV4tGnwvSU1oCuhoRgUZdcSz1",
    "12D3KooWB7NmMD96UiBDkbfpruTx8FJCxsWEyAkAnxJVrXDvCHmJ",
    "12D3KooWG4VfGiapcKqCYQa4FrKrjH3dYbDhGNWTEYEyCsm4LDWK",
    "12D3KooWGncVKp8YXAZyggxRBjnoWvkaTCxHQmHrmigvZZ7G8EC3",
    "12D3KooWPSGVzepsoCTzxXRgHucdBJGXAe2jmXwoHtPpupuHq2UU",
    "12D3KooWEFouDgtzcBB7djcHYyVY6vEnBpJppvC3YUaniMTp935c",
    "12D3KooWAVDM6uizMJyg8hwMuCwXAbAMHNfYqf5GgJtVPw8unuyN",
    "12D3KooWE1fSAsdaecmJfYyNhL5muX9WUQEbX6Zj2NeGz43g5Nne",
    "12D3KooWN544LYMCbuh92TosdSYn9iQZftZFieLfMyfb17EcMU5W",
    "12D3KooWNtnuHzEdN486neJ89nJ6ag4auwfjejeJ2uuENKREH8cG",
    "12D3KooWNamn29Cu6e9Dz7gCgZbkRxeJipPuXqh3aoMCBhkgfp6M",
    "12D3KooWKjgHJ5jk1i5sT9SKMD6WnVDo47UUvQRaZmDEeqtpHpgp",
    "12D3KooWSQpB7cU8vj2kZ5mzMvwfr26PGiUXKF4WvcMY8g3fHa58",
    "12D3KooWBALVNsm9vkWph12DVMxtqTY4kaevc571Jfi5kZkM2u3j",
    "12D3KooWCi4EiuWmsXEE29hjmzgac2LMsitV97dhHKsTSdmjri55",
    "12D3KooWQEZHJ6gNgT2S9pAkvniMp9LG65euofJfPWWQSrt3p2ob",
    "12D3KooWSKmJYCZ1G2hCbGWgKkAbwMjU7w3SQBnjyeBAFZfZAmsv",
    "12D3KooWF4rYCnUGjmwHhRthnL7VH9dB9DbQUBz3YkMjyB4irq9w",
    "12D3KooWB2HmUE2ZYZVQ8oZh47uTrfJq64L7XVWtzAAGgYV47HXP",
    "12D3KooWJjpjXX44VDVYLGaZf1RHMRZH4RY6NwBTwVAevgA2Ud89",
    "12D3KooWH48Yr24LZtrVWChAy4aob4iYuaLGxRVFysnfW4Wgcr3i",
    "12D3KooWNvSKNjYaXbj59vnk2YzMDY7XpjxmYECdh1jrGxMVaHDL",
    "12D3KooWPSpYeeES15D6zbkf37HRmvbrpTRtqxKtgFrTNCkcaw96",
    "12D3KooWFsQnPdqpbKDRpXxhTzDSHHb5QkzP1me3PGeoML6nHNgt",
];

pub(crate) fn get_peer_id(node_id: usize) -> PeerId {
    assert!(node_id < PEER_IDS.len(), "Node index out of bounds: {node_id}");
    PeerId::from_str(PEER_IDS[node_id]).unwrap()
}

pub(crate) fn peer_address(
    node_service: NodeService,
    port: u16,
    namespace: &str,
    peer_id: &PeerId,
    sanitized_domain: &str,
) -> Multiaddr {
    let dns = build_service_namespace_domain_address(
        &node_service.k8s_service_name(),
        namespace,
        sanitized_domain,
    );
    build_mutliaddr(&dns, port, peer_id)
}

fn build_mutliaddr(dns: &str, port: u16, peer_id: &PeerId) -> Multiaddr {
    Multiaddr::from_str(format!("/dns/{dns}").as_str())
        .unwrap()
        .with(Protocol::Tcp(port))
        .with(Protocol::P2p(*peer_id))
}

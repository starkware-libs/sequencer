use std::net::TcpListener;

pub fn find_free_port() -> u16 {
    // The socket is automatically closed when the function exits.
    // The port may still be available when accessed, but this is not guaranteed.
    // TODO(Asmaa): find a reliable way to ensure the port stays free.
    let listener = TcpListener::bind("0.0.0.0:0").expect("Failed to bind");
    listener.local_addr().expect("Failed to get local address").port()
}

pub fn find_n_free_ports(n: usize) -> Vec<u16> {
    // The socket is automatically closed when the function exits.
    // The port may still be available when accessed, but this is not guaranteed.
    // TODO(Asmaa): find a reliable way to ensure the port stays free.
    let mut ports = Vec::with_capacity(n);
    for _ in 0..n {
        let listener = TcpListener::bind("0.0.0.0:0").expect("Failed to bind");
        let port = listener.local_addr().expect("Failed to get local address").port();
        ports.push(port);
    }
    ports
}

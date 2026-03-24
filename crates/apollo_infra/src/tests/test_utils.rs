use std::net::SocketAddr;
use std::os::unix::io::BorrowedFd;
use std::time::Duration;

use socket2::SockRef;

/// Returns the `TCP_KEEPIDLE` duration of the outbound socket in this process that is connected
/// to `server_addr`, or `None` if no such socket is found or `SO_KEEPALIVE` is not enabled.
pub(crate) fn client_socket_keepalive_time(server_addr: SocketAddr) -> Option<Duration> {
    for fd in 0_i32..4096 {
        // SAFETY: We only borrow the fd transiently to read socket options; `SockRef` does
        // not take ownership of or close the fd. Invalid fds produce errors from `peer_addr`
        // and `keepalive`, which we handle gracefully via `.ok()` / `.unwrap_or`.
        let borrowed = unsafe { BorrowedFd::borrow_raw(fd) };
        let sock = SockRef::from(&borrowed);
        if sock
            .peer_addr()
            .ok()
            .and_then(|a: socket2::SockAddr| a.as_socket())
            .is_some_and(|a| a == server_addr)
        {
            return sock.keepalive().unwrap_or(false).then(|| sock.keepalive_time().ok()).flatten();
        }
    }
    None
}

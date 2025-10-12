use std::net::SocketAddr;

/// Bind to 127.0.0.1:0 to reserve an ephemeral port, return the SocketAddr and immediately
/// drop the listener so another server can bind it. This is a small race window helper for tests
/// that need to construct URLs before spawning the real server.
pub async fn reserve_ephemeral_port() -> anyhow::Result<SocketAddr> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    // Drop listener to free the port
    drop(listener);
    Ok(addr)
}

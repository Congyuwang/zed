//! socks proxy
use anyhow::{anyhow, Result};
use futures::io::{AsyncRead, AsyncWrite};
use http_client::proxy::Proxy;
use tokio_socks::{
    io::Compat,
    tcp::{Socks4Stream, Socks5Stream},
};

pub(crate) async fn connect_socks_proxy_stream(
    proxy: Proxy,
    rpc_host: (&str, u16),
) -> Result<Box<dyn AsyncReadWrite>> {
    let stream = match parse_socks_proxy(proxy) {
        Some((socks_proxy, SocksVersion::V4)) => {
            let stream = Socks4Stream::connect_with_socket(
                Compat::new(smol::net::TcpStream::connect(socks_proxy).await?),
                rpc_host,
            )
            .await
            .map_err(|err| anyhow!("error connecting to socks {}", err))?;
            Box::new(stream) as Box<dyn AsyncReadWrite>
        }
        Some((socks_proxy, SocksVersion::V5)) => Box::new(
            Socks5Stream::connect_with_socket(
                Compat::new(smol::net::TcpStream::connect(socks_proxy).await?),
                rpc_host,
            )
            .await
            .map_err(|err| anyhow!("error connecting to socks {}", err))?,
        ) as Box<dyn AsyncReadWrite>,
        None => Box::new(smol::net::TcpStream::connect(rpc_host).await?) as Box<dyn AsyncReadWrite>,
    };
    Ok(stream)
}

fn parse_socks_proxy(proxy: Proxy) -> Option<((String, u16), SocksVersion)> {
    let proxy_uri = proxy.to_uri()?;
    let scheme = proxy_uri.scheme_str()?;
    let socks_version = if scheme.starts_with("socks4") {
        // socks4
        SocksVersion::V4
    } else if scheme.starts_with("socks") {
        // socks, socks5
        SocksVersion::V5
    } else {
        return None;
    };
    if let (Some(host), Some(port)) = (proxy_uri.host(), proxy_uri.port_u16()) {
        Some(((host.to_string(), port), socks_version))
    } else {
        None
    }
}

enum SocksVersion {
    V4,
    V5,
}

pub(crate) trait AsyncReadWrite: AsyncRead + AsyncWrite + Unpin + Send + 'static {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send + 'static> AsyncReadWrite for T {}

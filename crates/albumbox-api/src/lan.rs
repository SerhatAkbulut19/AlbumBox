//! Rejects requests from outside the local network.

use std::net::{IpAddr, SocketAddr};

use axum::{
    extract::{ConnectInfo, Request},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};

pub fn is_local(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_private() || v4.is_loopback() || v4.is_link_local(),
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_local(IpAddr::V4(v4));
            }
            let first = v6.segments()[0];
            v6.is_loopback()
                || (first & 0xfe00) == 0xfc00 // unique local fc00::/7
                || (first & 0xffc0) == 0xfe80 // link-local fe80::/10
        }
    }
}

pub async fn lan_only(req: Request, next: Next) -> Response {
    let peer = req.extensions().get::<ConnectInfo<SocketAddr>>().map(|ci| ci.0.ip());
    match peer {
        Some(ip) if !is_local(ip) => {
            tracing::warn!(%ip, "rejected non-local request");
            StatusCode::FORBIDDEN.into_response()
        }
        _ => next.run(req).await,
    }
}

#[cfg(test)]
mod tests {
    use super::is_local;

    #[test]
    fn classifies_addresses() {
        for ip in ["192.168.1.5", "10.0.0.2", "172.16.4.1", "127.0.0.1", "::1", "fe80::1", "fd00::5", "::ffff:192.168.0.3"] {
            assert!(is_local(ip.parse().unwrap()), "{ip} should be local");
        }
        for ip in ["8.8.8.8", "2001:4860::8888", "::ffff:8.8.8.8"] {
            assert!(!is_local(ip.parse().unwrap()), "{ip} should be public");
        }
    }
}

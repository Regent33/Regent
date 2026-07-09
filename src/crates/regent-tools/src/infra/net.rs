//! Shared network helper: an SSRF-guarded HTTP GET used by both `web_fetch`
//! (text) and `vision_analyze` (image bytes). Redirects are followed manually
//! so every hop is re-validated against a private-IP denylist (loopback /
//! private / link-local incl. cloud metadata 169.254.169.254 / ULA /
//! unspecified / CGNAT); the body is read under a byte cap (memory-DoS guard).
//!
//! One implementation so the SSRF policy can never diverge between callers.

use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

const MAX_REDIRECTS: usize = 5;

/// GET `url` with an SSRF guard, returning `(status, body)` with the body
/// capped at `max_bytes`. Each redirect hop is re-validated; `timeout_secs`
/// bounds the whole request. The connection is pinned to the exact IP that
/// was validated (via `.resolve()`), so a host can't pass validation on one
/// DNS answer and connect to a private one on the next (DNS rebinding).
pub async fn guarded_get_bytes(
    url_str: &str,
    max_bytes: usize,
    timeout_secs: u64,
) -> Result<(u16, Vec<u8>), String> {
    let mut current = reqwest::Url::parse(url_str).map_err(|e| format!("bad url: {e}"))?;
    for _ in 0..=MAX_REDIRECTS {
        let validated_ip = validate_public_url(&current).await?;
        let host = current.host_str().ok_or("url has no host")?;
        let port = current.port_or_known_default().unwrap_or(80);
        // Per-hop client: `.resolve` pins host → validated IP for the connect
        // while SNI/Host headers keep the hostname (HTTPS still verifies).
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .user_agent("regent/0.1 (+https://github.com/regent)")
            .redirect(reqwest::redirect::Policy::none())
            .resolve(host, SocketAddr::new(validated_ip, port))
            .build()
            .map_err(|e| format!("http client error: {e}"))?;
        let resp = client
            .get(current.clone())
            .send()
            .await
            .map_err(|e| format!("fetch failed: {e}"))?;
        let status = resp.status();
        if status.is_redirection() {
            let location = resp
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .ok_or("redirect without a Location header")?;
            current = current
                .join(location)
                .map_err(|e| format!("bad redirect: {e}"))?;
            continue;
        }
        let body = read_capped(resp, max_bytes).await?;
        return Ok((status.as_u16(), body));
    }
    Err("too many redirects".into())
}

/// Reject anything but http(s) to a host that resolves only to public IPs.
/// Returns the validated address so the caller can pin the connection to it.
pub async fn validate_public_url(url: &reqwest::Url) -> Result<IpAddr, String> {
    let scheme = url.scheme();
    if scheme != "http" && scheme != "https" {
        return Err("only http(s) URLs are allowed".into());
    }
    let host = url.host_str().ok_or("url has no host")?;
    let port = url.port_or_known_default().unwrap_or(80);
    let addrs: Vec<IpAddr> = if let Ok(ip) = host.parse::<IpAddr>() {
        vec![ip]
    } else {
        tokio::net::lookup_host((host, port))
            .await
            .map_err(|e| format!("cannot resolve host: {e}"))?
            .map(|s| s.ip())
            .collect()
    };
    if let Some(ip) = addrs.iter().find(|ip| is_blocked_ip(ip)) {
        return Err(format!("refusing to reach internal/private address ({ip})"));
    }
    addrs
        .first()
        .copied()
        .ok_or_else(|| "host did not resolve".into())
}

/// True if `ip` is not a public, routable address (SSRF denylist).
pub fn is_blocked_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local() // 169.254.0.0/16 — incl. cloud metadata
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.octets()[0] == 0 // 0.0.0.0/8
                || (v4.octets()[0] == 100 && (v4.octets()[1] & 0xc0) == 64) // 100.64/10 CGNAT
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (v6.segments()[0] & 0xffc0) == 0xfe80 // link-local fe80::/10
                || (v6.octets()[0] & 0xfe) == 0xfc       // unique-local fc00::/7
                || v6.to_ipv4().is_some_and(|m| is_blocked_ip(&IpAddr::V4(m)))
        }
    }
}

/// Read a response body, stopping at `max_bytes` (memory-DoS guard).
pub async fn read_capped(mut resp: reqwest::Response, max_bytes: usize) -> Result<Vec<u8>, String> {
    let mut buf: Vec<u8> = Vec::new();
    while let Some(chunk) = resp
        .chunk()
        .await
        .map_err(|e| format!("read failed: {e}"))?
    {
        buf.extend_from_slice(&chunk);
        if buf.len() >= max_bytes {
            buf.truncate(max_bytes);
            break;
        }
    }
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ssrf_denylist_blocks_internal_addresses() {
        for ip in [
            "127.0.0.1",
            "10.0.0.5",
            "192.168.1.1",
            "172.16.9.9",
            "169.254.169.254", // cloud metadata
            "0.0.0.0",
            "100.64.0.1", // CGNAT
            "::1",
            "fe80::1",
            "fc00::1",
            "::ffff:127.0.0.1",
        ] {
            assert!(is_blocked_ip(&ip.parse().unwrap()), "should block {ip}");
        }
    }

    /// The validator returns the exact IP the connect will be pinned to
    /// (the DNS-rebinding guard), and still refuses blocked hosts.
    #[tokio::test]
    async fn validation_returns_pinnable_ip_and_refuses_private() {
        let url = reqwest::Url::parse("http://8.8.8.8/x").unwrap();
        let ip = validate_public_url(&url).await.unwrap();
        assert_eq!(ip, "8.8.8.8".parse::<IpAddr>().unwrap());

        let private = reqwest::Url::parse("http://169.254.169.254/latest").unwrap();
        assert!(validate_public_url(&private).await.is_err());
    }

    #[test]
    fn ssrf_denylist_allows_public_addresses() {
        for ip in ["8.8.8.8", "1.1.1.1", "93.184.216.34", "2606:2800:220:1::1"] {
            assert!(!is_blocked_ip(&ip.parse().unwrap()), "should allow {ip}");
        }
    }
}

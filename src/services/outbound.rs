//! Fetching an address somebody else typed.
//!
//! Three places take a URL from a member and go and read it: the personal
//! blog feed in `portfolio_sync`, the CI report in `quality_practice`, and
//! anything added later. A server that fetches what it is told to fetch is a
//! proxy into its own network — the request comes from inside, so a firewall
//! does not see it, and `http://169.254.169.254/` is a cloud provider's
//! credential endpoint on every major host.
//!
//! `fetch_feed` was already doing this with nothing but an `https://` check.
//! This module is what it should have been calling.
//!
//! ## What is refused
//!
//! Anything that is not `https`. Any host that resolves to an address inside
//! the machine, the network, or the ranges reserved for them: loopback,
//! private, link-local (which is where the metadata endpoints live),
//! carrier-grade NAT, unique-local, multicast and the unspecified address.
//! Every address a name resolves to is checked, not just the first — a name
//! with one public and one private answer is a name chosen to get through.
//!
//! ## Rebinding
//!
//! Checking the address and then letting the client resolve the name again is
//! two lookups, and a name that answers differently between them is the whole
//! attack. The request is pinned to the address that was checked, so the
//! second lookup does not happen.
//!
//! ## What is not defended here
//!
//! A public address that belongs to somebody else's private service. That is
//! not solvable at this layer, and the answer to it is that nothing fetched
//! this way is ever echoed back to the caller — the readers of this module
//! keep a count, not a body.

use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use std::time::Duration;

use crate::errors::AppError;

/// Long enough for a slow site on a bad connection, short enough that a sweep
/// of two hundred rows cannot be held open by one of them.
const TIMEOUT: Duration = Duration::from_secs(10);

/// Read at most this much. A CI report is kilobytes; a feed is tens of them.
/// Anything larger is either not what it claims or is being used to fill this
/// process's memory.
pub const MAX_BODY_BYTES: usize = 4 * 1024 * 1024;

/// Fetch a public URL as text, or say why not.
pub async fn get_text(url: &str) -> Result<String, AppError> {
    let (host, addr) = check(url)?;

    // Pinned to the address that was checked. `redirect::Policy::none()`
    // because a redirect is a second address nobody checked — a 3xx to
    // `http://169.254.169.254/` is the shortest version of this attack.
    let client = reqwest::Client::builder()
        .timeout(TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .resolve(&host, addr)
        .build()
        .map_err(|e| AppError::Internal(format!("outbound client: {e}")))?;

    let response = client
        .get(url)
        .header("User-Agent", "skilluv (https://skill-uv.com)")
        .send()
        .await
        .map_err(|e| AppError::Validation(format!("could not reach {host}: {e}")))?;

    if response.status().is_redirection() {
        return Err(AppError::Validation(
            "that address redirects, and the address it redirects to is one nobody \
             checked — give the final address instead"
                .into(),
        ));
    }

    let response = response
        .error_for_status()
        .map_err(|e| AppError::Validation(format!("{host} refused: {e}")))?;

    // Refuse on the declared length before reading, and again on what
    // actually arrived: a server may declare one length and send another.
    if let Some(len) = response.content_length()
        && len as usize > MAX_BODY_BYTES
    {
        return Err(AppError::Validation(format!(
            "that document is {len} bytes and the limit is {MAX_BODY_BYTES}"
        )));
    }

    let body = response
        .text()
        .await
        .map_err(|e| AppError::Validation(format!("{host} sent something unreadable: {e}")))?;

    if body.len() > MAX_BODY_BYTES {
        return Err(AppError::Validation(format!(
            "that document is larger than the {MAX_BODY_BYTES} byte limit"
        )));
    }

    Ok(body)
}

/// The host and the one address it is allowed to be reached at.
fn check(url: &str) -> Result<(String, SocketAddr), AppError> {
    let parsed = url::Url::parse(url)
        .map_err(|_| AppError::Validation(format!("`{url}` is not an address")))?;

    if parsed.scheme() != "https" {
        return Err(AppError::Validation(
            "the address has to be https — plain http would send whatever it carries \
             in the clear, and it is also how this check gets walked around"
                .into(),
        ));
    }

    let host = parsed
        .host_str()
        .ok_or_else(|| AppError::Validation("that address names no host".into()))?
        .to_string();
    let port = parsed.port_or_known_default().unwrap_or(443);

    // Resolution is blocking, and a sweep calls this a couple of hundred
    // times an hour at most.
    let host_for_lookup = host.clone();
    let resolved: Vec<SocketAddr> = (host_for_lookup.as_str(), port)
        .to_socket_addrs()
        .map_err(|e| AppError::Validation(format!("`{host}` does not resolve: {e}")))?
        .collect();

    if resolved.is_empty() {
        return Err(AppError::Validation(format!(
            "`{host}` resolves to nothing"
        )));
    }

    // Every answer, not the first. A name that answers with one public
    // address and one private one is a name chosen to get through.
    for addr in &resolved {
        if let Some(why) = why_refused(addr.ip()) {
            return Err(AppError::Validation(format!(
                "`{host}` resolves to {why}, which this server will not fetch"
            )));
        }
    }

    Ok((host, resolved[0]))
}

/// Why an address is out of bounds, or `None` if it is a public one.
pub fn why_refused(ip: IpAddr) -> Option<&'static str> {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            if v4.is_loopback() {
                Some("a loopback address")
            } else if v4.is_private() {
                Some("a private address")
            } else if v4.is_link_local() {
                // 169.254.169.254 is the credential endpoint on AWS, GCP,
                // Azure and DigitalOcean alike. It is the reason this
                // function exists.
                Some("a link-local address")
            } else if v4.is_unspecified() {
                Some("the unspecified address")
            } else if v4.is_broadcast() || v4.is_multicast() {
                Some("a broadcast or multicast address")
            } else if o[0] == 100 && (64..128).contains(&o[1]) {
                Some("a carrier-grade NAT address")
            } else if o[0] == 0 {
                Some("an address in 0.0.0.0/8")
            } else {
                None
            }
        }
        IpAddr::V6(v6) => {
            let seg = v6.segments();
            if v6.is_loopback() {
                Some("a loopback address")
            } else if v6.is_unspecified() {
                Some("the unspecified address")
            } else if v6.is_multicast() {
                Some("a multicast address")
            } else if seg[0] & 0xfe00 == 0xfc00 {
                Some("a unique-local address")
            } else if seg[0] & 0xffc0 == 0xfe80 {
                Some("a link-local address")
            } else if let Some(v4) = v6.to_ipv4_mapped() {
                // `::ffff:169.254.169.254` reaches the same place.
                why_refused(IpAddr::V4(v4))
            } else {
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    fn v4(a: u8, b: u8, c: u8, d: u8) -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(a, b, c, d))
    }

    #[test]
    fn the_metadata_endpoint_is_refused() {
        assert!(why_refused(v4(169, 254, 169, 254)).is_some());
        // And through the mapped form, which is the same machine.
        let mapped: Ipv6Addr = "::ffff:169.254.169.254".parse().unwrap();
        assert!(why_refused(IpAddr::V6(mapped)).is_some());
    }

    #[test]
    fn the_ranges_that_mean_inside_are_refused() {
        for ip in [
            v4(127, 0, 0, 1),
            v4(10, 0, 0, 5),
            v4(172, 16, 3, 1),
            v4(192, 168, 1, 1),
            v4(100, 64, 0, 1),
            v4(0, 0, 0, 0),
            v4(224, 0, 0, 1),
        ] {
            assert!(why_refused(ip).is_some(), "{ip} should be refused");
        }

        for raw in ["::1", "fc00::1", "fe80::1", "::"] {
            let ip: Ipv6Addr = raw.parse().unwrap();
            assert!(
                why_refused(IpAddr::V6(ip)).is_some(),
                "{raw} should be refused"
            );
        }
    }

    #[test]
    fn a_public_address_is_allowed() {
        assert!(why_refused(v4(1, 1, 1, 1)).is_none());
        assert!(why_refused(v4(140, 82, 121, 4)).is_none());
        let ip: Ipv6Addr = "2606:4700:4700::1111".parse().unwrap();
        assert!(why_refused(IpAddr::V6(ip)).is_none());
    }

    #[test]
    fn plain_http_is_refused_before_anything_is_resolved() {
        let err = check("http://example.com/report.xml").unwrap_err();
        assert!(format!("{err:?}").contains("https"));
    }

    #[test]
    fn something_that_is_not_an_address_is_refused() {
        assert!(check("not a url").is_err());
        assert!(check("file:///etc/passwd").is_err());
    }

    #[test]
    fn localhost_is_refused_by_name_as_well_as_by_number() {
        // The name resolves, and every address it resolves to is loopback.
        let err = check("https://localhost/report.xml").unwrap_err();
        assert!(format!("{err:?}").contains("loopback"), "{err:?}");
    }
}

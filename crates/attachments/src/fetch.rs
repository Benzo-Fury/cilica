//! Downloading an image from a web link without becoming a way into this computer or its network.
//!
//! A link pasted into a note is untrusted. Before any byte is sent, every hop is checked:
//! - only `http` and `https`, and no user name or password in the link;
//! - the host is resolved here, and every address it resolves to must be on the public internet
//!   (no loopback, private, link-local, carrier-grade NAT, unique-local, multicast, reserved, or an
//!   IPv6 form that wraps one of those);
//! - the connection is pinned to those vetted addresses through the client's resolver, so a second
//!   DNS answer (rebinding) can't send it elsewhere, and no proxy is used;
//! - redirects are followed by hand, at most five, each checked like the first link;
//! - GET only, no cookies, no credentials, no referrer, a generic `User-Agent`;
//! - one deadline covers everything, and the body is read in chunks and dropped past the cap,
//!   whatever `Content-Length` says.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use cilica_domain::{AppError, AppResult};
use futures::StreamExt;
use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use reqwest::header::{ACCEPT, LOCATION};
use url::{Host, Url};

use crate::format::MAX_INPUT_BYTES;

// ---------- Limits ---------- //

/// How far a download may go.
#[derive(Debug, Clone, Copy)]
pub struct FetchLimits {
    /// Most bytes read from a body.
    pub max_bytes: usize,
    /// One deadline for resolving, connecting, every redirect and the whole body.
    pub timeout: Duration,
    /// Most redirects followed.
    pub max_redirects: u8,
}

impl Default for FetchLimits {
    fn default() -> Self {
        Self { max_bytes: MAX_INPUT_BYTES, timeout: Duration::from_secs(15), max_redirects: 5 }
    }
}

// ---------- Addresses ---------- //

/// Whether an address is on the public internet, the only place Cilica downloads from.
pub fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_public_v4(v4),
        IpAddr::V6(v6) => is_public_v6(v6),
    }
}

/// IPv4: refuse this network, loopback, RFC 1918, CGNAT, link-local (cloud metadata), protocol
/// assignments, documentation, benchmarking, multicast, reserved and broadcast.
fn is_public_v4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    let refused = a == 0                                   // 0.0.0.0/8 "this network"
        || a == 10                                         // 10.0.0.0/8
        || a == 127                                        // loopback
        || (a == 100 && (64..=127).contains(&b))           // 100.64.0.0/10 CGNAT
        || (a == 169 && b == 254)                          // link-local, incl. 169.254.169.254
        || (a == 172 && (16..=31).contains(&b))            // 172.16.0.0/12
        || (a == 192 && b == 0 && c == 0)                  // 192.0.0.0/24 protocol assignments
        || (a == 192 && b == 0 && c == 2)                  // TEST-NET-1
        || (a == 192 && b == 88 && c == 99)                // 6to4 relay anycast
        || (a == 192 && b == 168)                          // 192.168.0.0/16
        || (a == 198 && (b == 18 || b == 19))              // benchmarking
        || (a == 198 && b == 51 && c == 100)               // TEST-NET-2
        || (a == 203 && b == 0 && c == 113)                // TEST-NET-3
        || a >= 224; // multicast, reserved 240/4 and broadcast
    !refused
}

/// IPv6: refuse unspecified, loopback, unique-local, link- and site-local, multicast,
/// documentation, discard, Teredo and ORCHID; addresses that wrap an IPv4 one (mapped, compatible,
/// NAT64, 6to4) are judged by the IPv4 address inside.
fn is_public_v6(ip: Ipv6Addr) -> bool {
    let s = ip.segments();

    // Forms that carry an IPv4 address: judge that address
    if let Some(v4) = ip.to_ipv4_mapped() {
        return is_public_v4(v4);
    }
    if s[..6] == [0, 0, 0, 0, 0, 0] {
        // ::/96 IPv4-compatible (also covers :: and ::1)
        return !ip.is_unspecified() && !ip.is_loopback() && is_public_v4(Ipv4Addr::from(((s[6] as u32) << 16) | s[7] as u32));
    }
    if s[..6] == [0x64, 0xff9b, 0, 0, 0, 0] {
        // 64:ff9b::/96 NAT64
        return is_public_v4(Ipv4Addr::from(((s[6] as u32) << 16) | s[7] as u32));
    }
    if s[0] == 0x2002 {
        // 2002::/16 6to4
        return is_public_v4(Ipv4Addr::from(((s[1] as u32) << 16) | s[2] as u32));
    }

    // Ranges that are never the public internet
    let refused = (s[0] & 0xfe00) == 0xfc00                // fc00::/7 unique-local
        || (s[0] & 0xffc0) == 0xfe80                       // fe80::/10 link-local
        || (s[0] & 0xffc0) == 0xfec0                       // fec0::/10 site-local (deprecated)
        || (s[0] & 0xff00) == 0xff00                       // multicast
        || (s[0] == 0x2001 && s[1] == 0x0db8)              // documentation
        || (s[0] == 0x2001 && s[1] == 0)                   // Teredo 2001::/32
        || (s[0] == 0x2001 && (s[1] & 0xfff0) == 0x0010)   // ORCHID 2001:10::/28
        || (s[0] == 0x2001 && (s[1] & 0xfff0) == 0x0020)   // ORCHIDv2 2001:20::/28
        || (s[0] == 0x0064 && s[1] == 0xff9b && s[2] == 1) // 64:ff9b:1::/48 local NAT64
        || (s[0] == 0x0100 && s[1..4] == [0, 0, 0]); // 100::/64 discard
    !refused
}

/// Pins the client to the addresses vetted for one host: any other name fails to resolve, so
/// nothing can reach an address that wasn't checked.
struct PinnedResolver {
    host: String,
    addrs: Vec<SocketAddr>,
}

impl Resolve for PinnedResolver {
    /// The vetted addresses for the vetted host; an error for any other name.
    fn resolve(&self, name: Name) -> Resolving {
        let found = if name.as_str().trim_end_matches('.').eq_ignore_ascii_case(&self.host) {
            Ok(Box::new(self.addrs.clone().into_iter()) as Addrs)
        } else {
            Err(format!("{} was not vetted", name.as_str()).into())
        };
        Box::pin(async move { found })
    }
}

// ---------- Fetcher ---------- //

/// The bytes a link answered with.
#[derive(Debug)]
pub struct Downloaded {
    pub bytes: Vec<u8>,
    /// Where the bytes came from after redirects (its last path segment names the image).
    pub url: Url,
}

/// Downloads images from web links under the rules at the top of this module. Cheap to clone.
#[derive(Clone)]
pub struct WebFetcher {
    limits: FetchLimits,
    /// Which addresses may be connected to. [`is_public`] outside tests.
    allowed: Arc<dyn Fn(IpAddr) -> bool + Send + Sync>,
}

impl Default for WebFetcher {
    fn default() -> Self {
        Self::new(FetchLimits::default())
    }
}

impl WebFetcher {
    /// A fetcher that only reaches public addresses.
    pub fn new(limits: FetchLimits) -> Self {
        Self { limits, allowed: Arc::new(is_public) }
    }

    /// A fetcher with its own address rule, so tests can reach a server on loopback.
    #[cfg(test)]
    pub(crate) fn allowing(limits: FetchLimits, allowed: impl Fn(IpAddr) -> bool + Send + Sync + 'static) -> Self {
        Self { limits, allowed: Arc::new(allowed) }
    }

    /// Download `link`, following up to the redirect limit, within the deadline.
    pub async fn get(&self, link: &str) -> AppResult<Downloaded> {
        let url = Url::parse(link.trim()).map_err(|_| AppError::invalid("That doesn't look like a web link. Check it and try again."))?;
        match tokio::time::timeout(self.limits.timeout, self.follow(url)).await {
            Ok(done) => done,
            Err(_) => Err(failed("the site took too long to answer. Try again or save the image and drag it in.")),
        }
    }

    /// Request each hop in turn, vetting it first, until one answers with a body.
    async fn follow(&self, mut url: Url) -> AppResult<Downloaded> {
        for hop in 0..=self.limits.max_redirects {
            // Vet the link and pin the connection to the vetted addresses
            check_link(&url)?;
            let client = self.client_for(&url).await?;

            // Send one plain GET
            let response = client.get(url.clone()).header(ACCEPT, "image/*").send().await.map_err(|e| request_error(&e))?;
            let status = response.status();

            // Redirects are followed here, checked like the first link
            if status.is_redirection() {
                let location = response.headers().get(LOCATION).and_then(|v| v.to_str().ok()).map(str::to_string);
                let Some(location) = location else {
                    return Err(failed("the site sent Cilica somewhere without saying where. Save the image and drag it in instead."));
                };
                if hop == self.limits.max_redirects {
                    break;
                }
                url = url.join(&location).map_err(|_| failed("the site redirected to a link Cilica can't read. Save the image and drag it in instead."))?;
                continue;
            }

            // Anything but success is the site's answer to report
            if !status.is_success() {
                return Err(status_error(status.as_u16()));
            }
            let bytes = self.read_body(response).await?;
            return Ok(Downloaded { bytes, url });
        }
        Err(failed("the link redirected too many times. Save the image and drag it in instead."))
    }

    /// A client that can only connect to `url`'s host at addresses checked against the rule.
    async fn client_for(&self, url: &Url) -> AppResult<reqwest::Client> {
        let port = url.port_or_known_default().ok_or_else(|| failed("the link has no port Cilica can use."))?;

        // Resolve here (or take the literal address) and check every answer
        let (host, addrs): (String, Vec<SocketAddr>) = match url.host() {
            Some(Host::Ipv4(ip)) => (ip.to_string(), vec![SocketAddr::new(IpAddr::V4(ip), port)]),
            Some(Host::Ipv6(ip)) => (format!("[{ip}]"), vec![SocketAddr::new(IpAddr::V6(ip), port)]),
            Some(Host::Domain(domain)) => {
                let domain = domain.trim_end_matches('.').to_ascii_lowercase();
                let addrs: Vec<SocketAddr> = tokio::net::lookup_host((domain.as_str(), port))
                    .await
                    .map_err(|_| failed("Cilica couldn't find that site. Check the link and your connection, then try again."))?
                    .collect();
                (domain, addrs)
            }
            None => return Err(AppError::invalid("That link has no site in it. Check it and try again.")),
        };
        if addrs.is_empty() {
            return Err(failed("Cilica couldn't find that site. Check the link and your connection, then try again."));
        }
        if addrs.iter().any(|a| !(self.allowed)(a.ip())) {
            return Err(failed("the link points at this computer or a private network, which Cilica doesn't download from. Save the image and drag it in instead."));
        }

        // No proxy, no automatic redirects, no referrer; resolver pinned to the vetted addresses
        reqwest::Client::builder()
            .user_agent("Cilica")
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .referer(false)
            .connect_timeout(self.limits.timeout)
            .dns_resolver(Arc::new(PinnedResolver { host, addrs }))
            .build()
            .map_err(|e| AppError::internal(format!("Cilica couldn't start the download: {e}")))
    }

    /// Read the body in chunks, stopping as soon as it passes the cap.
    async fn read_body(&self, response: reqwest::Response) -> AppResult<Vec<u8>> {
        let too_big = || failed("the image is larger than 40 MB. Save a smaller copy and drag it in.");

        // A declared length over the cap ends it early; a smaller one is not trusted
        if response.content_length().is_some_and(|n| n > self.limits.max_bytes as u64) {
            return Err(too_big());
        }
        let mut body = Vec::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| request_error(&e))?;
            if body.len() + chunk.len() > self.limits.max_bytes {
                return Err(too_big());
            }
            body.extend_from_slice(&chunk);
        }
        Ok(body)
    }
}

/// Only http and https, with no user name or password (they would be sent as credentials).
fn check_link(url: &Url) -> AppResult<()> {
    if !matches!(url.scheme(), "http" | "https") {
        return Err(AppError::invalid("Cilica can only download images from http and https links."));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(AppError::invalid("Cilica doesn't download from links that carry a user name or password."));
    }
    Ok(())
}

/// A download error, worded for people.
fn failed(why: &str) -> AppError {
    AppError::upstream(format!("Cilica couldn't download that image: {why}"))
}

/// What a non-success status means for the person.
fn status_error(status: u16) -> AppError {
    match status {
        401 | 403 => failed("the site wouldn't share it. Open it in your browser, save the image and drag it in."),
        404 | 410 => failed("the site says it isn't there. Check the link and try again."),
        429 => failed("the site is busy. Wait a moment and try again."),
        _ => failed(&format!("the site answered with an error ({status}). Try again or save the image and drag it in.")),
    }
}

/// A transport error, worded for people (the details go to the log, not the message).
fn request_error(error: &reqwest::Error) -> AppError {
    tracing::debug!(%error, "image download failed");
    if error.is_timeout() {
        failed("the site took too long to answer. Try again or save the image and drag it in.")
    } else if error.is_connect() {
        failed("Cilica couldn't reach the site. Check your connection and try again.")
    } else {
        failed("the connection broke off. Try again or save the image and drag it in.")
    }
}

#[cfg(test)]
mod tests {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use super::*;

    /// A tiny HTTP server on loopback that answers each connection with the next canned reply.
    /// `{port}` in a reply is replaced with the server's port.
    async fn serve(replies: Vec<Vec<u8>>) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            for reply in replies {
                let Ok((mut socket, _)) = listener.accept().await else { return };
                let mut buf = [0u8; 4096];
                let _ = socket.read(&mut buf).await;
                let reply = String::from_utf8_lossy(&reply).replace("{port}", &port.to_string()).into_bytes();
                let _ = socket.write_all(&reply).await;
                let _ = socket.shutdown().await;
            }
        });
        port
    }

    /// An HTTP/1.1 reply with headers and a body.
    fn reply(status: &str, headers: &[&str], body: &[u8]) -> Vec<u8> {
        let mut out = format!("HTTP/1.1 {status}\r\nConnection: close\r\n").into_bytes();
        for h in headers {
            out.extend_from_slice(format!("{h}\r\n").as_bytes());
        }
        out.extend_from_slice(b"\r\n");
        out.extend_from_slice(body);
        out
    }

    /// A fetcher that may only reach 127.0.0.1 (not ::1, not 127.0.0.2).
    fn loopback_only(limits: FetchLimits) -> WebFetcher {
        WebFetcher::allowing(limits, |ip| ip == IpAddr::V4(Ipv4Addr::LOCALHOST))
    }

    #[test]
    fn tells_public_addresses_from_the_rest() {
        let refused = [
            "0.0.0.0", "127.0.0.1", "127.8.8.8", "10.1.2.3", "172.16.0.1", "172.31.255.255", "192.168.1.1", "169.254.169.254",
            "100.64.0.1", "100.127.255.254", "224.0.0.1", "239.255.255.250", "255.255.255.255", "240.0.0.1", "192.0.2.5",
            "198.18.0.1", "::", "::1", "fc00::1", "fd12:3456::1", "fe80::1", "fec0::1", "ff02::1", "::ffff:127.0.0.1",
            "::ffff:10.0.0.1", "::ffff:169.254.169.254", "::127.0.0.1", "64:ff9b::a00:1", "2002:7f00:1::", "2002:c0a8:101::1",
            "2001:db8::1", "2001::1",
        ];
        for ip in refused {
            assert!(!is_public(ip.parse().unwrap()), "{ip} should be refused");
        }
        let allowed = ["1.1.1.1", "8.8.8.8", "93.184.215.14", "172.32.0.1", "100.128.0.1", "2606:4700:4700::1111", "::ffff:1.1.1.1", "64:ff9b::808:808"];
        for ip in allowed {
            assert!(is_public(ip.parse().unwrap()), "{ip} should be allowed");
        }
    }

    #[test]
    fn only_plain_web_links() {
        for link in ["ftp://example.com/a.png", "file:///etc/passwd", "data:image/png;base64,AAAA", "https://user:pw@example.com/a.png"] {
            assert!(check_link(&Url::parse(link).unwrap()).is_err(), "{link}");
        }
        assert!(check_link(&Url::parse("https://example.com/a.png").unwrap()).is_ok());
    }

    #[tokio::test]
    async fn refuses_this_computer_and_private_networks() {
        let fetcher = WebFetcher::default();
        for link in [
            "http://127.0.0.1/a.png",
            "http://localhost/a.png",
            "http://[::1]/a.png",
            "http://[::ffff:127.0.0.1]/a.png",
            "http://169.254.169.254/latest/meta-data/",
            "http://10.0.0.1/a.png",
            "http://2130706433/a.png",
            "http://0x7f.1/a.png",
        ] {
            let error = fetcher.get(link).await.unwrap_err();
            assert!(error.message.contains("private network"), "{link}: {}", error.message);
        }
    }

    #[tokio::test]
    async fn downloads_an_image() {
        let port = serve(vec![reply("200 OK", &["Content-Type: text/plain"], b"bytes")]).await;
        let got = loopback_only(FetchLimits::default()).get(&format!("http://127.0.0.1:{port}/pic.png")).await.unwrap();
        assert_eq!(got.bytes, b"bytes");
    }

    #[tokio::test]
    async fn follows_redirects_and_checks_each_hop() {
        // One redirect is fine
        let port = serve(vec![reply("302 Found", &["Location: /next.png"], b""), reply("200 OK", &[], b"ok")]).await;
        let got = loopback_only(FetchLimits::default()).get(&format!("http://127.0.0.1:{port}/a")).await.unwrap();
        assert_eq!(got.bytes, b"ok");
        assert_eq!(got.url.path(), "/next.png");

        // A redirect to an address that isn't allowed is refused before connecting
        let port = serve(vec![reply("301 Moved", &["Location: http://127.0.0.2:{port}/x.png"], b"")]).await;
        let error = loopback_only(FetchLimits::default()).get(&format!("http://127.0.0.1:{port}/a")).await.unwrap_err();
        assert!(error.message.contains("private network"), "{}", error.message);

        // localhost also resolves to ::1, which isn't allowed, so it is refused as a whole
        let port = serve(vec![reply("302 Found", &["Location: http://localhost:{port}/x.png"], b"")]).await;
        assert!(loopback_only(FetchLimits::default()).get(&format!("http://127.0.0.1:{port}/a")).await.is_err());

        // A redirect to another scheme is refused
        let port = serve(vec![reply("302 Found", &["Location: file:///etc/passwd"], b"")]).await;
        let error = loopback_only(FetchLimits::default()).get(&format!("http://127.0.0.1:{port}/a")).await.unwrap_err();
        assert!(error.message.contains("http and https"), "{}", error.message);
    }

    #[tokio::test]
    async fn stops_after_five_redirects() {
        let hops = (0..6).map(|i| reply("302 Found", &[&format!("Location: /{i}")], b"")).collect();
        let port = serve(hops).await;
        let error = loopback_only(FetchLimits::default()).get(&format!("http://127.0.0.1:{port}/start")).await.unwrap_err();
        assert!(error.message.contains("too many times"), "{}", error.message);
    }

    #[tokio::test]
    async fn caps_the_body_whatever_the_length_says() {
        let limits = FetchLimits { max_bytes: 1000, ..FetchLimits::default() };

        // No length given: read until past the cap, then stop
        let port = serve(vec![reply("200 OK", &[], &[7u8; 5000])]).await;
        let error = loopback_only(limits).get(&format!("http://127.0.0.1:{port}/big")).await.unwrap_err();
        assert!(error.message.contains("larger than"), "{}", error.message);

        // A declared length over the cap is refused straight away
        let port = serve(vec![reply("200 OK", &["Content-Length: 999999"], &[7u8; 10])]).await;
        assert!(loopback_only(limits).get(&format!("http://127.0.0.1:{port}/big")).await.is_err());
    }

    #[tokio::test]
    async fn gives_up_on_a_slow_site() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            // Accept and never answer
            let (_socket, _) = listener.accept().await.unwrap();
            tokio::time::sleep(Duration::from_secs(30)).await;
        });
        let limits = FetchLimits { timeout: Duration::from_millis(300), ..FetchLimits::default() };
        let error = loopback_only(limits).get(&format!("http://127.0.0.1:{port}/slow")).await.unwrap_err();
        assert!(error.message.contains("took too long"), "{}", error.message);
    }

    #[tokio::test]
    async fn reports_missing_images() {
        let port = serve(vec![reply("404 Not Found", &[], b"")]).await;
        let error = loopback_only(FetchLimits::default()).get(&format!("http://127.0.0.1:{port}/gone.png")).await.unwrap_err();
        assert!(error.message.contains("isn't there"), "{}", error.message);
    }
}

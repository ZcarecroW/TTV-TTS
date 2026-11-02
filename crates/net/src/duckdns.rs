//! DuckDNS: a free, fixed name (`name.duckdns.org`) for a home connection
//! whose address changes. One HTTPS call keeps the name pointing at this
//! network; another sets the name's TXT record, which is how Let's Encrypt
//! checks that we own the name before it issues a certificate.
//!
//! The token goes in the URL (that's DuckDNS's API), so errors never show
//! the URL.

use std::time::Duration;

/// The DuckDNS API (a test can point it elsewhere in debug builds).
fn base() -> String {
    if cfg!(debug_assertions)
        && let Ok(u) = std::env::var("TTV_DUCKDNS_URL")
    {
        return u.trim_end_matches('/').to_string();
    }
    "https://www.duckdns.org".into()
}

fn web() -> reqwest::Client {
    reqwest::Client::builder().timeout(Duration::from_secs(20)).user_agent(concat!("TTV-TTS/", env!("CARGO_PKG_VERSION"))).build().expect("web client")
}

/// "name" from "name", "name.duckdns.org" or "https://name.duckdns.org/".
pub fn clean_domain(s: &str) -> String {
    let s = s.trim().trim_start_matches("https://").trim_start_matches("http://");
    let s = s.split(['/', ':']).next().unwrap_or_default();
    let s = s.strip_suffix(".duckdns.org").unwrap_or(s);
    s.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').take(63).collect::<String>().to_ascii_lowercase()
}

/// The full name: "name.duckdns.org".
pub fn fqdn(domain: &str) -> String {
    format!("{domain}.duckdns.org")
}

/// Whether `host` ("x.duckdns.org", maybe with a port) is a DuckDNS name.
pub fn is_duckdns(host: &str) -> bool {
    let h = host.trim().trim_start_matches("https://").trim_start_matches("http://");
    let h = h.split(['/', ':']).next().unwrap_or_default().to_ascii_lowercase();
    h.ends_with(".duckdns.org") && !clean_domain(&h).is_empty()
}

async fn call(query: &str) -> Result<Vec<String>, String> {
    let body = web()
        .get(format!("{}/update?{query}", base()))
        .send()
        .await
        .map_err(|e| format!("DuckDNS didn't answer: {}", e.without_url()))?
        .text()
        .await
        .map_err(|e| format!("DuckDNS: {}", e.without_url()))?;
    let lines: Vec<String> = body.lines().map(|l| l.trim().to_string()).collect();
    match lines.first().map(String::as_str) {
        Some("OK") => Ok(lines[1..].to_vec()),
        _ => Err("DuckDNS said KO: check the name and the token.".into()),
    }
}

/// Percent-encodes everything but unreserved characters.
fn url_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Points the name at the address DuckDNS sees this request coming from.
/// Returns that public IPv4 address.
pub async fn update_ip(domain: &str, token: &str) -> Result<Option<String>, String> {
    let lines = call(&format!("domains={}&token={}&verbose=true", url_escape(domain), url_escape(token))).await?;
    Ok(lines.first().filter(|s| !s.is_empty()).cloned())
}

/// Sets the name's TXT record (Let's Encrypt's DNS-01 challenge).
pub async fn set_txt(domain: &str, token: &str, value: &str) -> Result<(), String> {
    call(&format!("domains={}&token={}&txt={}&verbose=true", url_escape(domain), url_escape(token), url_escape(value))).await.map(|_| ())
}

/// Removes the TXT record again.
pub async fn clear_txt(domain: &str, token: &str) -> Result<(), String> {
    call(&format!("domains={}&token={}&txt=removed&clear=true", url_escape(domain), url_escape(token))).await.map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(clean_domain(" https://My-Stream.duckdns.org:8112/o/x "), "my-stream");
        assert_eq!(clean_domain("mystream"), "mystream");
        assert_eq!(fqdn("mystream"), "mystream.duckdns.org");
        assert!(is_duckdns("mystream.duckdns.org"));
        assert!(is_duckdns("MyStream.DuckDNS.org:8112"));
        assert!(!is_duckdns("mystream.example.com"));
        assert!(!is_duckdns("203.0.113.9"));
        assert!(!is_duckdns(".duckdns.org"));
    }

    #[test]
    fn escapes() {
        assert_eq!(url_escape("a-b_c.d~e"), "a-b_c.d~e");
        assert_eq!(url_escape("a b&c=d+/"), "a%20b%26c%3Dd%2B%2F");
    }
}

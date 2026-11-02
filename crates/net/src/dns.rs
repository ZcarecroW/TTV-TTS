//! A tiny DNS client for one question: does a name's TXT record show a
//! value yet? Asked straight at the zone's own (authoritative) name servers,
//! so no cache answers with an old value. Let's Encrypt looks the TXT record
//! up the same way, so once these servers have it, validation can start.

use std::net::SocketAddr;
use std::time::Duration;

use tokio::net::UdpSocket;

/// The TXT strings of `name` according to `server` (empty: no such record).
pub async fn txt(server: SocketAddr, name: &str) -> Result<Vec<String>, String> {
    let sock = UdpSocket::bind(if server.is_ipv4() { "0.0.0.0:0" } else { "[::]:0" }).await.map_err(|e| e.to_string())?;
    sock.connect(server).await.map_err(|e| e.to_string())?;
    let id: u16 = rand::random();
    let q = query(id, name)?;
    let mut buf = vec![0u8; 4096];
    for _ in 0..3 {
        sock.send(&q).await.map_err(|e| e.to_string())?;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        while let Ok(Ok(n)) = tokio::time::timeout_at(deadline, sock.recv(&mut buf)).await {
            // an answer to another question (or a stray packet) is ignored
            if let Some(r) = answer(&buf[..n], id) {
                return r;
            }
        }
    }
    Err(format!("the name server {server} didn't answer"))
}

/// A query for the TXT record of `name`, without recursion.
fn query(id: u16, name: &str) -> Result<Vec<u8>, String> {
    let mut q = Vec::with_capacity(name.len() + 18);
    q.extend(id.to_be_bytes());
    q.extend([0, 0, 0, 1, 0, 0, 0, 0, 0, 0]); // flags, 1 question, no answers
    for label in name.trim_end_matches('.').split('.') {
        if label.is_empty() || label.len() > 63 {
            return Err(format!("not a valid name: {name}"));
        }
        q.push(label.len() as u8);
        q.extend(label.as_bytes());
    }
    q.push(0);
    q.extend([0, 16, 0, 1]); // TXT, IN
    Ok(q)
}

/// The TXT strings of a response to query `id`; `None` when it isn't one.
fn answer(b: &[u8], id: u16) -> Option<Result<Vec<String>, String>> {
    if b.len() < 12 || u16::from_be_bytes([b[0], b[1]]) != id || b[2] & 0x80 == 0 {
        return None;
    }
    match b[3] & 0x0f {
        0 => {}
        3 => return Some(Ok(Vec::new())), // no such name
        code => return Some(Err(format!("the name server answered with error {code}"))),
    }
    let questions = u16::from_be_bytes([b[4], b[5]]);
    let answers = u16::from_be_bytes([b[6], b[7]]);
    let mut i = 12;
    for _ in 0..questions {
        i = skip_name(b, i)? + 4;
    }
    let mut out = Vec::new();
    for _ in 0..answers {
        i = skip_name(b, i)?;
        let kind = u16::from_be_bytes([*b.get(i)?, *b.get(i + 1)?]);
        let len = u16::from_be_bytes([*b.get(i + 8)?, *b.get(i + 9)?]) as usize;
        i += 10;
        let data = b.get(i..i + len)?;
        i += len;
        if kind != 16 {
            continue;
        }
        // one or more <length><text> strings make up one record
        let (mut s, mut j) = (String::new(), 0);
        while j < data.len() {
            let n = data[j] as usize;
            s.push_str(&String::from_utf8_lossy(data.get(j + 1..j + 1 + n)?));
            j += 1 + n;
        }
        out.push(s);
    }
    Some(Ok(out))
}

/// Past a (possibly compressed) name.
fn skip_name(b: &[u8], mut i: usize) -> Option<usize> {
    loop {
        let l = *b.get(i)?;
        if l == 0 {
            return Some(i + 1);
        }
        if l & 0xc0 == 0xc0 {
            return Some(i + 2);
        }
        i += 1 + l as usize;
    }
}

/// The addresses of DuckDNS's own name servers (ns1 … ns9.duckdns.org).
pub async fn duckdns_servers() -> Vec<SocketAddr> {
    let mut out = Vec::new();
    for n in 1..=9 {
        if let Ok(mut a) = tokio::net::lookup_host((format!("ns{n}.duckdns.org"), 53)).await
            && let Some(a) = a.find(SocketAddr::is_ipv4)
        {
            out.push(a);
        }
    }
    out
}

/// Waits until every one of `servers` (up to 4 are asked) has `value` in
/// the TXT record of `name`, at most `limit`. True when they all have it.
pub async fn wait_for_txt(servers: &[SocketAddr], name: &str, value: &str, limit: Duration) -> bool {
    let ask: Vec<SocketAddr> = servers.iter().copied().take(4).collect();
    if ask.is_empty() {
        tokio::time::sleep(limit.min(Duration::from_secs(30))).await;
        return false;
    }
    let end = tokio::time::Instant::now() + limit;
    loop {
        let mut all = true;
        for s in &ask {
            if !txt(*s, name).await.is_ok_and(|v| v.iter().any(|t| t == value)) {
                all = false;
                break;
            }
        }
        if all {
            return true;
        }
        if tokio::time::Instant::now() + Duration::from_secs(3) > end {
            return false;
        }
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_a_query() {
        let q = query(0x1234, "_acme-challenge.my.duckdns.org").unwrap();
        assert_eq!(&q[..4], &[0x12, 0x34, 0, 0]);
        assert_eq!(&q[12..28], b"\x0f_acme-challenge");
        assert_eq!(&q[q.len() - 4..], &[0, 16, 0, 1]);
        assert!(query(1, "a..b").is_err());
    }

    #[test]
    fn reads_an_answer_with_compression_and_split_strings() {
        let q = query(7, "x.duckdns.org").unwrap();
        let mut r = q.clone();
        r[2] = 0x84; // response, authoritative
        r[7] = 2; // two answers
        // answer 1: pointer to the question name, TXT, "abc" + "def"
        r.extend([0xc0, 12, 0, 16, 0, 1, 0, 0, 0, 60, 0, 8, 3, b'a', b'b', b'c', 3, b'd', b'e', b'f']);
        // answer 2: an A record (skipped)
        r.extend([0xc0, 12, 0, 1, 0, 1, 0, 0, 0, 60, 0, 4, 1, 2, 3, 4]);
        assert_eq!(answer(&r, 7), Some(Ok(vec!["abcdef".to_string()])));
        assert_eq!(answer(&r, 8), None, "another id");
        let mut nx = q.clone();
        nx[2] = 0x84;
        nx[3] = 3;
        assert_eq!(answer(&nx, 7), Some(Ok(vec![])));
        assert_eq!(answer(&r[..r.len() - 30], 7), None, "cut short");
    }

    #[test]
    fn hostile_answers_never_panic() {
        let q = query(7, "x.duckdns.org").unwrap();
        let mut x: u64 = 0x0bad_c0de_dead_beef;
        for _ in 0..50_000 {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            let mut r = q.clone();
            r[2] = 0x84;
            r.extend((0..(x % 80)).map(|i| (x >> (i % 56)) as u8));
            for k in 0..(x % 5) {
                let i = ((x >> (7 * k)) as usize) % r.len();
                r[i] = (x >> (5 * k)) as u8;
            }
            let _ = answer(&r, u16::from_be_bytes([r[0], r[1]]));
        }
    }

    /// Live: asks DuckDNS's name servers (needs the internet).
    #[tokio::test]
    #[ignore]
    async fn duckdns_name_servers_answer() {
        let servers = duckdns_servers().await;
        println!("{servers:?}");
        assert!(servers.len() >= 3);
        let r = txt(servers[0], "_acme-challenge.ttv-tts-nonexistent-name-for-test.duckdns.org").await;
        println!("{r:?}");
        assert!(r.is_ok());
    }
}

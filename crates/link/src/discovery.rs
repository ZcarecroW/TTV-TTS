//! LAN discovery: the app sends [`REQUEST`] as a UDP broadcast to port
//! [`crate::DEFAULT_PORT`]; every proxy answers with [`REPLY_PREFIX`] followed
//! by its [`ProxyInfo`] as JSON. Nothing secret is exchanged: pairing still
//! needs the code shown on the proxy, and the reply's fingerprint is checked
//! again in the TLS handshake.

use crate::protocol::ProxyInfo;

pub const REQUEST: &[u8] = b"TTVTTS-DISCOVER/1";
pub const REPLY_PREFIX: &[u8] = b"TTVTTS-PROXY/1 ";
/// Replies larger than this are ignored (a reply is ~400 bytes).
pub const MAX_PACKET: usize = 2048;

pub fn is_request(packet: &[u8]) -> bool {
    packet == REQUEST || packet.strip_suffix(b"\n") == Some(REQUEST)
}

pub fn encode_reply(info: &ProxyInfo) -> Vec<u8> {
    let mut v = REPLY_PREFIX.to_vec();
    v.extend_from_slice(&serde_json::to_vec(info).unwrap_or_default());
    v
}

pub fn decode_reply(packet: &[u8]) -> Option<ProxyInfo> {
    if packet.len() > MAX_PACKET {
        return None;
    }
    let json = packet.strip_prefix(REPLY_PREFIX)?;
    let info: ProxyInfo = serde_json::from_slice(json).ok()?;
    (!info.id.is_empty() && info.port != 0).then_some(info)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let info = ProxyInfo { id: "abc".into(), name: "VCP-PC".into(), port: 8110, ..Default::default() };
        assert_eq!(decode_reply(&encode_reply(&info)), Some(info));
        assert!(is_request(b"TTVTTS-DISCOVER/1"));
        assert!(!is_request(b"TTVTTS-DISCOVER/2"));
        assert_eq!(decode_reply(b"garbage"), None);
        assert_eq!(decode_reply(b"TTVTTS-PROXY/1 {\"id\":\"\"}"), None);
    }
}

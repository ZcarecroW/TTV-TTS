//! Just enough DER to read what renewing needs from a certificate: when it
//! is valid, the names it is for, its serial number and the issuer's key id
//! (both identify it for ACME renewal information, RFC 9773).

/// What a certificate says about itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Facts {
    /// Unix seconds.
    pub not_before: i64,
    pub not_after: i64,
    /// DNS names (subject alternative names).
    pub names: Vec<String>,
    /// Serial number: the INTEGER's content bytes.
    pub serial: Vec<u8>,
    /// Authority key identifier: the keyIdentifier's content bytes.
    pub aki: Option<Vec<u8>>,
}

struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    /// The next tag and its contents.
    fn next(&mut self) -> Option<(u8, &'a [u8])> {
        let (&tag, rest) = self.0.split_first()?;
        let (&l0, mut rest) = rest.split_first()?;
        let len = if l0 < 0x80 {
            l0 as usize
        } else {
            let n = (l0 & 0x7f) as usize;
            if n == 0 || n > 4 || rest.len() < n {
                return None;
            }
            let l = rest[..n].iter().fold(0usize, |l, &b| (l << 8) | b as usize);
            rest = &rest[n..];
            l
        };
        if rest.len() < len {
            return None;
        }
        let (v, after) = rest.split_at(len);
        self.0 = after;
        Some((tag, v))
    }

    fn expect(&mut self, tag: u8) -> Option<&'a [u8]> {
        let (t, v) = self.next()?;
        (t == tag).then_some(v)
    }

    fn peek(&self) -> Option<u8> {
        self.0.first().copied()
    }
}

const SEQUENCE: u8 = 0x30;
const OID_SAN: &[u8] = &[0x55, 0x1d, 0x11];
const OID_AKI: &[u8] = &[0x55, 0x1d, 0x23];

/// Reads a DER certificate (the first one of a chain).
pub fn facts(cert: &[u8]) -> Option<Facts> {
    let mut outer = Reader(Reader(cert).expect(SEQUENCE)?);
    let mut tbs = Reader(outer.expect(SEQUENCE)?);
    if tbs.peek() == Some(0xa0) {
        tbs.next()?; // version
    }
    let serial = tbs.expect(0x02)?.to_vec();
    tbs.expect(SEQUENCE)?; // signature algorithm
    tbs.expect(SEQUENCE)?; // issuer
    let mut validity = Reader(tbs.expect(SEQUENCE)?);
    let not_before = time(validity.next()?)?;
    let not_after = time(validity.next()?)?;
    tbs.expect(SEQUENCE)?; // subject
    tbs.expect(SEQUENCE)?; // public key
    let (mut names, mut aki) = (Vec::new(), None);
    while let Some((tag, v)) = tbs.next() {
        if tag != 0xa3 {
            continue; // issuer/subject unique ids
        }
        let mut list = Reader(Reader(v).expect(SEQUENCE)?);
        while let Some(ext) = list.expect(SEQUENCE) {
            let mut ext = Reader(ext);
            let oid = ext.expect(0x06)?;
            if ext.peek() == Some(0x01) {
                ext.next()?; // critical
            }
            let value = ext.expect(0x04)?;
            if oid == OID_SAN {
                let mut gn = Reader(Reader(value).expect(SEQUENCE)?);
                while let Some((t, n)) = gn.next() {
                    if t == 0x82 {
                        names.push(String::from_utf8_lossy(n).to_ascii_lowercase());
                    }
                }
            } else if oid == OID_AKI {
                let mut a = Reader(Reader(value).expect(SEQUENCE)?);
                while let Some((t, k)) = a.next() {
                    if t == 0x80 {
                        aki = Some(k.to_vec());
                    }
                }
            }
        }
    }
    Some(Facts { not_before, not_after, names, serial, aki })
}

/// UTCTime ("YYMMDDHHMMSSZ") or GeneralizedTime ("YYYYMMDDHHMMSSZ") → unix seconds.
fn time((tag, v): (u8, &[u8])) -> Option<i64> {
    let s = std::str::from_utf8(v).ok().filter(|s| s.is_ascii())?.strip_suffix('Z')?;
    let (year, rest) = match tag {
        0x17 if s.len() == 12 => {
            let yy: i64 = s[..2].parse().ok()?;
            (if yy >= 50 { 1900 + yy } else { 2000 + yy }, &s[2..])
        }
        0x18 if s.len() == 14 => (s[..4].parse().ok()?, &s[4..]),
        _ => return None,
    };
    let n = |i: usize| rest.get(i..i + 2)?.parse::<i64>().ok();
    let (month, day, h, m, sec) = (n(0)?, n(2)?, n(4)?, n(6)?, n(8)?);
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || h > 23 || m > 59 || sec > 60 {
        return None;
    }
    Some(days_from_civil(year, month, day) * 86_400 + h * 3600 + m * 60 + sec)
}

/// Days since 1970-01-01 (proleptic Gregorian calendar).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calendar() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2000, 3, 1), 11_017);
        assert_eq!(days_from_civil(2026, 11, 2), 20_759);
        assert_eq!(time((0x17, b"261102120000Z")), Some(20_759 * 86_400 + 12 * 3600));
        assert_eq!(time((0x18, b"20501231235959Z")), Some(days_from_civil(2050, 12, 31) * 86_400 + 86_399));
        assert_eq!(time((0x17, b"2611021200Z")), None);
    }

    #[test]
    fn reads_a_certificate() {
        let mut p = rcgen::CertificateParams::new(vec!["mystream.duckdns.org".to_string()]).unwrap();
        p.not_before = rcgen::date_time_ymd(2026, 11, 2);
        p.not_after = rcgen::date_time_ymd(2027, 1, 31);
        p.serial_number = Some(rcgen::SerialNumber::from_slice(&[0x01, 0x23, 0x45]));
        p.use_authority_key_identifier_extension = true;
        let ca_key = rcgen::KeyPair::generate().unwrap();
        let mut ca = rcgen::CertificateParams::new(Vec::<String>::new()).unwrap();
        ca.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
        ca.key_identifier_method = rcgen::KeyIdMethod::PreSpecified(vec![9, 8, 7, 6]);
        let issuer = rcgen::Issuer::new(ca, &ca_key);
        let key = rcgen::KeyPair::generate().unwrap();
        let cert = p.signed_by(&key, &issuer).unwrap();
        let f = facts(cert.der()).unwrap();
        assert_eq!(f.names, vec!["mystream.duckdns.org"]);
        assert_eq!(f.not_before, days_from_civil(2026, 11, 2) * 86_400);
        assert_eq!(f.not_after, days_from_civil(2027, 1, 31) * 86_400);
        assert_eq!(f.serial, vec![0x01, 0x23, 0x45]);
        assert_eq!(f.aki, Some(vec![9, 8, 7, 6]));
        assert!(facts(&cert.der()[..40]).is_none());
    }

    /// Broken or hostile bytes never panic: a real certificate with bytes
    /// flipped, cut short, and odd times.
    #[test]
    fn hostile_bytes_never_panic() {
        let key = rcgen::KeyPair::generate().unwrap();
        let cert = rcgen::CertificateParams::new(vec!["x.duckdns.org".to_string()]).unwrap().self_signed(&key).unwrap();
        let good = cert.der().to_vec();
        assert!(facts(&good).is_some());
        let mut x: u64 = 0x1234_5678_9abc_def1;
        for round in 0..20_000 {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            let mut b = good.clone();
            for k in 0..(1 + x % 6) {
                let i = ((x >> (8 * k)) as usize) % b.len();
                b[i] = (x >> (3 * k)) as u8;
            }
            if round % 5 == 0 {
                b.truncate((x % good.len() as u64) as usize);
            }
            let _ = facts(&b);
        }
        for t in ["2611021200Z", "26110212000éZ", "ééééééZ", "999999999999Z", "Z", ""] {
            let _ = time((0x17, t.as_bytes()));
            let _ = time((0x18, t.as_bytes()));
        }
        assert_eq!(time((0x17, "26110212000éZ".as_bytes())), None, "non-ASCII");
    }
}

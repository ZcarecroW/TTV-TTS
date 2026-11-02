// Scrambling of the built-in Twitch client ID. Shared by build.rs (which
// scrambles the CI secret with a new random seed for every build) and the
// app (which unscrambles it in memory when a login needs it), so the ID
// never shows up as text in the binary or in a strings dump.
//
// Layout: blob[0] = length ^ k, then the ID's bytes in shuffled order, each
// XORed with the next keystream byte, then keystream noise up to 64 bytes.

#[allow(dead_code)]
const BLOB_LEN: usize = 64;

/// splitmix64 keystream.
#[allow(dead_code)]
fn next(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Positions of the ID's bytes in the blob (Fisher–Yates from the keystream).
#[allow(dead_code)]
fn order(n: usize, state: &mut u64) -> Vec<usize> {
    let mut p: Vec<usize> = (0..n).collect();
    for i in (1..n).rev() {
        let j = (next(state) % (i as u64 + 1)) as usize;
        p.swap(i, j);
    }
    p
}

#[allow(dead_code)]
pub fn scramble(id: &str, seed: u64) -> Vec<u8> {
    let bytes = id.as_bytes();
    let n = bytes.len().min(BLOB_LEN - 1);
    let mut s = seed;
    let mut blob = vec![n as u8 ^ next(&mut s) as u8];
    let p = order(n, &mut s);
    for &i in &p {
        blob.push(bytes[i] ^ next(&mut s) as u8);
    }
    while blob.len() < BLOB_LEN {
        blob.push(next(&mut s) as u8);
    }
    blob
}

#[allow(dead_code)]
pub fn unscramble(blob: &[u8], seed: u64) -> Option<String> {
    let mut s = seed;
    let n = (*blob.first()? ^ next(&mut s) as u8) as usize;
    if n == 0 || n >= blob.len() {
        return None;
    }
    let p = order(n, &mut s);
    let mut out = vec![0u8; n];
    for (k, &i) in p.iter().enumerate() {
        out[i] = blob[k + 1] ^ next(&mut s) as u8;
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unscrambles_what_was_scrambled_and_hides_it() {
        let id = "abcdefghijklmnopqrstuvwxyz0123";
        for seed in [1u64, 42, u64::MAX, 0x1234_5678_9abc_def0] {
            let blob = scramble(id, seed);
            assert_eq!(blob.len(), 64);
            assert_eq!(unscramble(&blob, seed).as_deref(), Some(id));
            // no piece of the ID shows as text
            assert!(!blob.windows(4).any(|w| id.as_bytes().windows(4).any(|x| x == w)), "seed {seed}");
        }
        // another seed gives other bytes
        assert_ne!(scramble(id, 1), scramble(id, 2));
    }
}

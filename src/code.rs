//! Short codes for maze seeds: four characters from an alphabet with no
//! look-alikes (no I, L, O, 0 or 1), easy to read off a page, say aloud and
//! type back in.

use crate::rng::Rng;

const ALPHABET: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";
const BASE: u64 = ALPHABET.len() as u64;
/// Seeds that fit in four characters.
pub const SPACE: u64 = BASE * BASE * BASE * BASE;

/// The code for a seed: four characters, more for seeds beyond SPACE.
pub fn encode(mut seed: u64) -> String {
    let mut out = Vec::new();
    while out.len() < 4 || seed > 0 {
        out.push(ALPHABET[(seed % BASE) as usize]);
        seed /= BASE;
    }
    out.reverse();
    String::from_utf8(out).unwrap()
}

/// The seed for a code (any case). All digits is read as a plain number,
/// for seeds printed before codes; fresh codes always have a letter.
pub fn decode(code: &str) -> Option<u64> {
    if !code.is_empty() && code.bytes().all(|b| b.is_ascii_digit()) {
        return code.parse().ok();
    }
    code.bytes().try_fold(0u64, |seed, b| {
        let i = ALPHABET.iter().position(|&a| a == b.to_ascii_uppercase())?;
        seed.checked_mul(BASE)?.checked_add(i as u64)
    })
}

fn has_letter(seed: u64) -> bool {
    encode(seed).bytes().any(|b| b.is_ascii_alphabetic())
}

/// A random seed whose code has a letter in it.
pub fn fresh(rng: &mut Rng) -> u64 {
    loop {
        let seed = rng.below(SPACE as usize) as u64;
        if has_letter(seed) {
            return seed;
        }
    }
}

/// The next seed after this one whose code has a letter in it.
pub fn next(seed: u64) -> u64 {
    let mut s = (seed + 1) % SPACE;
    while !has_letter(s) {
        s = (s + 1) % SPACE;
    }
    s
}

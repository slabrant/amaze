use crate::seeded_rng::Rng;

const ALPHABET_WITHOUT_LOOK_ALIKES: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";
const BASE: u64 = ALPHABET_WITHOUT_LOOK_ALIKES.len() as u64;
const CODE_LENGTH: usize = 4;
pub const SEEDS_WITH_FOUR_CHARACTER_CODES: u64 = BASE * BASE * BASE * BASE;

pub fn encode(mut seed: u64) -> String {
    let mut out = Vec::new();
    while out.len() < CODE_LENGTH || seed > 0 {
        out.push(ALPHABET_WITHOUT_LOOK_ALIKES[(seed % BASE) as usize]);
        seed /= BASE;
    }
    out.reverse();
    String::from_utf8(out).unwrap()
}

fn is_plain_number_from_before_codes(code: &str) -> bool {
    !code.is_empty() && code.bytes().all(|b| b.is_ascii_digit())
}

pub fn decode(code: &str) -> Option<u64> {
    if is_plain_number_from_before_codes(code) {
        return code.parse().ok();
    }
    code.bytes().try_fold(0u64, |seed, b| {
        let digit = ALPHABET_WITHOUT_LOOK_ALIKES.iter().position(|&a| a == b.to_ascii_uppercase())?;
        seed.checked_mul(BASE)?.checked_add(digit as u64)
    })
}

fn code_has_a_letter(seed: u64) -> bool {
    encode(seed).bytes().any(|b| b.is_ascii_alphabetic())
}

pub fn random_seed_with_a_letter(rng: &mut Rng) -> u64 {
    loop {
        let seed = rng.below(SEEDS_WITH_FOUR_CHARACTER_CODES as usize) as u64;
        if code_has_a_letter(seed) {
            return seed;
        }
    }
}

pub fn next_seed_with_a_letter(seed: u64) -> u64 {
    let mut next = (seed + 1) % SEEDS_WITH_FOUR_CHARACTER_CODES;
    while !code_has_a_letter(next) {
        next = (next + 1) % SEEDS_WITH_FOUR_CHARACTER_CODES;
    }
    next
}

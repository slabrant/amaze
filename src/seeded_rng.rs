pub struct Xoshiro256StarStar {
    state: [u64; 4],
}

pub type Rng = Xoshiro256StarStar;

fn splitmix64_state(seed: u64) -> [u64; 4] {
    let mut x = seed;
    let mut state = [0; 4];
    for word in &mut state {
        x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = x;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        *word = z ^ (z >> 31);
    }
    state
}

impl Xoshiro256StarStar {
    pub fn new(seed: u64) -> Self {
        Xoshiro256StarStar { state: splitmix64_state(seed) }
    }

    pub fn next_u64(&mut self) -> u64 {
        let s = &mut self.state;
        let result = s[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = s[1] << 17;
        s[2] ^= s[0];
        s[3] ^= s[1];
        s[1] ^= s[2];
        s[0] ^= s[3];
        s[2] ^= t;
        s[3] = s[3].rotate_left(45);
        result
    }

    pub fn from_zero_to_one(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    pub fn below(&mut self, n: usize) -> usize {
        ((self.next_u64() as u128 * n as u128) >> 64) as usize
    }

    pub fn shuffle<T>(&mut self, v: &mut [T]) {
        for i in (1..v.len()).rev() {
            v.swap(i, self.below(i + 1));
        }
    }

    pub fn index_by_weight(&mut self, weights: &[f64]) -> usize {
        let total: f64 = weights.iter().sum();
        let mut x = self.from_zero_to_one() * total;
        for (i, &weight) in weights.iter().enumerate() {
            if x < weight {
                return i;
            }
            x -= weight;
        }
        weights.len() - 1
    }
}

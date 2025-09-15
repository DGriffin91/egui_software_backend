pub struct Hash32(pub u32);

impl Hash32 {
    #[inline(always)]
    pub fn new_fnv() -> Self {
        Hash32(0x811c9dc5) // FNV offset basis
    }

    #[inline(always)]
    pub fn hash_wrap(&mut self, v: u32) {
        self.hash(v);
        self.fnv_wrap();
    }

    #[inline(always)]
    pub fn hash(&mut self, v: u32) {
        self.0 ^= v;
    }

    #[inline(always)]
    pub fn fnv_wrap(&mut self) {
        self.0 = self.0.wrapping_mul(0x01000193); // FNV prime
    }

    pub fn finalize(&self) -> u32 {
        self.0
    }
}

#[inline(always)]
pub fn hash32(x: u32) -> u32 {
    // from https://nullprogram.com/blog/2018/07/31/
    let mut x = x ^ (x >> 16);
    x = x.overflowing_mul(0x7feb352d).0;
    x = x ^ (x >> 15);
    x = x.overflowing_mul(0x846ca68b).0;
    x = x ^ (x >> 16);
    x
}

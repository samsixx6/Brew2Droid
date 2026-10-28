//! SHA-1 de fluxo para o IHash legado (RFC 3174, seções 4 a 6).
//! O jogo usa o resumo real para conferir seus próprios blocos de código.

#[derive(Debug, Clone)]
pub struct Sha1 {
    state: [u32; 5],
    buffer: Vec<u8>,
    length: u64,
}

impl Default for Sha1 {
    fn default() -> Self {
        Self { state: [0x67452301, 0xefcdab89, 0x98badcfe, 0x10325476, 0xc3d2e1f0],
            buffer: Vec::new(), length: 0 }
    }
}

impl Sha1 {
    pub fn estado(&self) -> ([u32; 5], &[u8], u64) { (self.state, &self.buffer, self.length) }

    pub fn restaura_estado(&mut self, state: [u32; 5], buffer: &[u8], length: u64) -> Result<(), String> {
        if buffer.len() >= 64 || length % 64 != buffer.len() as u64 {
            return Err("resto SHA-1 incompatível com o comprimento".into());
        }
        self.state = state;
        self.buffer = buffer.to_vec();
        self.length = length;
        Ok(())
    }

    pub fn update(&mut self, mut data: &[u8]) {
        self.length = self.length.wrapping_add(data.len() as u64);
        if !self.buffer.is_empty() {
            let count = (64 - self.buffer.len()).min(data.len());
            self.buffer.extend_from_slice(&data[..count]);
            data = &data[count..];
            if self.buffer.len() < 64 { return; }
            let block: [u8; 64] = self.buffer.as_slice().try_into().unwrap();
            self.compress(&block);
            self.buffer.clear();
        }
        let mut blocks = data.chunks_exact(64);
        for block in &mut blocks { self.compress(block.try_into().unwrap()); }
        self.buffer.extend_from_slice(blocks.remainder());
    }

    pub fn finish(mut self) -> [u8; 20] {
        let bits = self.length.wrapping_mul(8);
        self.buffer.push(0x80);
        while self.buffer.len() % 64 != 56 { self.buffer.push(0); }
        self.buffer.extend_from_slice(&bits.to_be_bytes());
        let padded = std::mem::take(&mut self.buffer);
        for block in padded.chunks_exact(64) { self.compress(block.try_into().unwrap()); }
        let mut digest = [0; 20];
        for (word, bytes) in self.state.iter().zip(digest.chunks_exact_mut(4)) {
            bytes.copy_from_slice(&word.to_be_bytes());
        }
        digest
    }

    fn compress(&mut self, block: &[u8; 64]) {
        let mut words = [0u32; 80];
        for (index, bytes) in block.chunks_exact(4).enumerate() {
            words[index] = u32::from_be_bytes(bytes.try_into().unwrap());
        }
        for index in 16..80 {
            words[index] = (words[index-3] ^ words[index-8] ^ words[index-14] ^ words[index-16]).rotate_left(1);
        }
        let [mut a, mut b, mut c, mut d, mut e] = self.state;
        for (index, word) in words.into_iter().enumerate() {
            let (function, constant) = match index {
                0..=19 => ((b & c) | (!b & d), 0x5a827999u32),
                20..=39 => (b ^ c ^ d, 0x6ed9eba1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8f1bbcdc),
                _ => (b ^ c ^ d, 0xca62c1d6),
            };
            let next = a.rotate_left(5).wrapping_add(function).wrapping_add(e)
                .wrapping_add(constant).wrapping_add(word);
            e = d; d = c; c = b.rotate_left(30); b = a; a = next;
        }
        for (state, result) in self.state.iter_mut().zip([a,b,c,d,e]) { *state = state.wrapping_add(result); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn hex(bytes: &[u8]) -> String { bytes.iter().map(|byte| format!("{byte:02x}")).collect() }

    #[test]
    fn rfc3174_vectors_and_every_split_across_two_blocks() {
        for (input, expected) in [
            (b"abc".as_slice(), "a9993e364706816aba3e25717850c26c9cd0d89d"),
            (b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq".as_slice(), "84983e441c3bd26ebaae4aa1f95129e5e54670f1"),
        ] {
            for split in 0..=input.len() {
                let mut hash = Sha1::default(); hash.update(&input[..split]);
                let (state, buffer, length) = hash.estado();
                let mut restored = Sha1::default(); restored.restaura_estado(state, buffer, length).unwrap();
                restored.update(&input[split..]);
                assert_eq!(hex(&restored.finish()), expected);
            }
        }
        let mut hash = Sha1::default();
        for _ in 0..1000 { hash.update(&[b'a'; 1000]); }
        assert_eq!(hex(&hash.finish()), "34aa973cd4c4daa4f61eeb2bdbad27316534016f");
    }

    #[test]
    fn rejects_impossible_saved_remainders() {
        let mut hash = Sha1::default(); let state = hash.state;
        assert!(hash.restaura_estado(state, &[0;64],64).is_err());
        assert!(hash.restaura_estado(state, &[0;2],1).is_err());
    }
}

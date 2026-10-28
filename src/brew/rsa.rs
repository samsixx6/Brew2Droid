//! Exponenciação modular e PKCS#1 v1.5 do IRSA legado (RFC 8017).
//! Sem biblioteca do host: os números entram e saem em big-endian, como no SDK.

pub const MAX_BYTES: usize = 512;
const MAX_WORK: u64 = 64_000_000;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Key,
    Size,
    Unsupported,
    Padding,
    Signature,
}

pub fn valid_key(modulus: &[u8], exponent: &[u8]) -> bool {
    !modulus.is_empty()
        && modulus.len() <= MAX_BYTES
        && !exponent.is_empty()
        && exponent.len() <= MAX_BYTES
        && (modulus[..modulus.len() - 1].iter().any(|byte| *byte != 0)
            || modulus[modulus.len() - 1] > 1)
        && exponent.iter().any(|byte| *byte != 0)
}

fn words(bytes: &[u8], count: usize) -> Vec<u32> {
    let mut result = vec![0; count];
    for (index, byte) in bytes.iter().rev().enumerate() {
        result[index / 4] |= u32::from(*byte) << ((index % 4) * 8);
    }
    result
}
fn less(a: &[u32], b: &[u32]) -> bool {
    a.iter().rev().cmp(b.iter().rev()).is_lt()
}
fn add_mod(a: &[u32], b: &[u32], modulus: &[u32], result: &mut [u32]) {
    let mut carry = 0u64;
    for ((a, b), out) in a.iter().zip(b).zip(result.iter_mut()) {
        let sum = u64::from(*a) + u64::from(*b) + carry;
        *out = sum as u32;
        carry = sum >> 32;
    }
    // A soma é menor que 2*n. Um carry de largura fixa também exige subtrair n;
    // o empréstimo final absorve esse carry sem perder o bit mais significativo.
    if carry != 0 || !less(result, modulus) {
        let mut borrow = 0u64;
        for (out, n) in result.iter_mut().zip(modulus) {
            let sub = u64::from(*n) + borrow;
            let value = u64::from(*out);
            *out = value.wrapping_sub(sub) as u32;
            borrow = u64::from(value < sub);
        }
    }
}
fn multiply(a: &[u32], b: &[u32], modulus: &[u32]) -> Vec<u32> {
    let mut result = vec![0; modulus.len()];
    let mut addend = a.to_vec();
    let mut scratch = result.clone();
    for word in b {
        let mut bits = *word;
        for _ in 0..32 {
            if bits & 1 != 0 {
                add_mod(&result, &addend, modulus, &mut scratch);
                std::mem::swap(&mut result, &mut scratch);
            }
            add_mod(&addend, &addend, modulus, &mut scratch);
            std::mem::swap(&mut addend, &mut scratch);
            bits >>= 1;
        }
    }
    result
}

pub fn mod_exp(modulus: &[u8], exponent: &[u8], input: &[u8]) -> Result<Vec<u8>, Error> {
    if !valid_key(modulus, exponent) {
        return Err(Error::Key);
    }
    if input.len() != modulus.len() {
        return Err(Error::Size);
    }
    let count = modulus.len().div_ceil(4);
    let n = words(modulus, count);
    let base = words(input, count);
    if !less(&base, &n) {
        return Err(Error::Size);
    }
    let first = exponent.iter().position(|byte| *byte != 0).unwrap();
    let bits = (exponent.len() - first) * 8 - exponent[first].leading_zeros() as usize;
    // Chaves públicas usuais ficam muito abaixo do teto. Uma chave externa com
    // expoente denso não pode bloquear o laço móvel por trabalho ilimitado.
    if (bits as u64) * (modulus.len() as u64 * 8) * (count as u64) > MAX_WORK {
        return Err(Error::Unsupported);
    }
    let mut result = vec![0; count];
    result[0] = 1;
    let mut started = false;
    for byte in &exponent[first..] {
        for bit in (0..8).rev() {
            let set = byte & (1 << bit) != 0;
            if !started && !set {
                continue;
            }
            started = true;
            result = multiply(&result, &result, &n);
            if set {
                result = multiply(&result, &base, &n);
            }
        }
    }
    let mut out = vec![0; modulus.len()];
    for (index, byte) in out.iter_mut().rev().enumerate() {
        *byte = (result[index / 4] >> ((index % 4) * 8)) as u8;
    }
    Ok(out)
}

pub fn verify_digest(
    modulus: &[u8],
    exponent: &[u8],
    signature: &[u8],
    digest: &[u8],
    kind: u32,
) -> Result<(), Error> {
    let (prefix, length): (&[u8], usize) = match kind {
        2 => (
            &[
                0x30, 0x20, 0x30, 0x0c, 0x06, 0x08, 0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x02, 0x05,
                0x05, 0x00, 0x04, 0x10,
            ],
            16,
        ),
        3 => (
            &[
                0x30, 0x21, 0x30, 0x09, 0x06, 0x05, 0x2b, 0x0e, 0x03, 0x02, 0x1a, 0x05, 0x00, 0x04,
                0x14,
            ],
            20,
        ),
        _ => return Err(Error::Unsupported),
    };
    if digest.len() != length || signature.is_empty() || signature.len() > modulus.len() {
        return Err(Error::Size);
    }
    let mut input = vec![0; modulus.len()];
    input[modulus.len() - signature.len()..].copy_from_slice(signature);
    let encoded = mod_exp(modulus, exponent, &input)?;
    if !encoded.starts_with(&[0, 1]) {
        return Err(Error::Padding);
    }
    let separator = encoded[2..]
        .iter()
        .position(|byte| *byte == 0)
        .map(|pos| pos + 2)
        .ok_or(Error::Padding)?;
    if separator < 10 || encoded[2..separator].iter().any(|byte| *byte != 0xff) {
        return Err(Error::Padding);
    }
    let info = &encoded[separator + 1..];
    if info.len() != prefix.len() + length
        || !info.starts_with(prefix)
        || &info[prefix.len()..] != digest
    {
        return Err(Error::Signature);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    include!("../../tests/fixtures/handset_rsa_vectors.rs");
    #[test]
    fn independent_python_big_integer_vector_and_classic_example() {
        assert_eq!(
            mod_exp(&[0x0c, 0xa1], &[17], &[0, 65]).unwrap(),
            [0x0a, 0xe6]
        );
        assert_eq!(mod_exp(N, E, SIG_SHA1).unwrap(), ENCODED_SHA1);
        assert_eq!(verify_digest(N, E, SIG_SHA1, SHA1, 3), Ok(()));
        assert_eq!(verify_digest(N, E, SIG_MD5, MD5, 2), Ok(()));
        assert_eq!(
            verify_digest(N, E, SIG_BAD_PADDING, SHA1, 3),
            Err(Error::Padding)
        );
    }
    #[test]
    fn carries_and_partial_top_words_match_native_u128_arithmetic() {
        let mut seed = 0x123456789abcdefu64;
        for length in [1usize, 2, 3, 4, 5, 7, 8] {
            for _ in 0..20 {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                let mask = u64::MAX >> (64 - length * 8);
                let n = (seed & mask).max(3);
                let a = seed.rotate_left(29) % n;
                let mut answer = 1u128;
                let mut base = u128::from(a);
                let mut exp = 65537;
                while exp != 0 {
                    if exp & 1 != 0 {
                        answer = answer * base % u128::from(n);
                    }
                    base = base * base % u128::from(n);
                    exp >>= 1;
                }
                let actual = mod_exp(
                    &n.to_be_bytes()[8 - length..],
                    &[1, 0, 1],
                    &a.to_be_bytes()[8 - length..],
                )
                .unwrap();
                assert_eq!(actual, &(answer as u64).to_be_bytes()[8 - length..]);
            }
        }
    }
    #[test]
    fn invalid_keys_ranges_and_unbounded_work_are_rejected() {
        assert_eq!(mod_exp(&[0], &[3], &[0]), Err(Error::Key));
        assert_eq!(mod_exp(&[3], &[0], &[1]), Err(Error::Key));
        assert_eq!(mod_exp(&[3], &[1], &[3]), Err(Error::Size));
        assert_eq!(mod_exp(&[3], &[1], &[0, 1]), Err(Error::Size));
        assert_eq!(
            mod_exp(&vec![0xff; 512], &vec![0xff; 512], &vec![1; 512]),
            Err(Error::Unsupported)
        );
    }
}

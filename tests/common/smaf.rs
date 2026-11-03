pub(crate) fn chunk(id: [u8; 4], body: &[u8]) -> Vec<u8> {
    [id.as_slice(), &(body.len() as u32).to_be_bytes(), body].concat()
}
pub(crate) fn crc(bytes: &[u8]) -> u16 {
    let mut r = 0xffffu16;
    for &b in bytes {
        r ^= u16::from(b) << 8;
        for _ in 0..8 {
            r = if r & 0x8000 != 0 {
                (r << 1) ^ 0x1021
            } else {
                r << 1
            };
        }
    }
    !r
}
pub(crate) fn seal(mut body: Vec<u8>) -> Vec<u8> {
    body.extend([0, 0]);
    let mut bytes = chunk(*b"MMMD", &body);
    let n = bytes.len();
    let c = crc(&bytes[..n - 2]);
    bytes[n - 2..].copy_from_slice(&c.to_be_bytes());
    bytes
}
pub(crate) fn track(seq: &[u8], wave: &[u8], format: u8, base: u8) -> Vec<u8> {
    let mut body = vec![0, 0, format, 0, base, base];
    body.extend(chunk(*b"Atsq", seq));
    body.extend(chunk([b'A', b'w', b'a', 1], wave));
    chunk([b'A', b'T', b'R', 0], &body)
}
pub(crate) fn file(seq: &[u8], wave: &[u8]) -> Vec<u8> {
    seal([chunk(*b"CNTI", &[0, 1, 2, 0, 0]), track(seq, wave, 0x11, 0)].concat())
}
pub(crate) fn left(seq: &[u8]) -> Vec<u8> {
    [vec![0, 0, 0x3a, 0], seq.to_vec()].concat()
}
pub(crate) fn pcm(v: f32) -> i32 {
    (v * 32768.0).round() as i32
}

pub fn encode_varint(mut value: u32) -> ([u8; 5], usize) {
    let mut buffer = [0u8; 5];
    let mut i = 0;
    while value >= 0x80 {
        buffer[i] = (value as u8 & 0x7F) | 0x80;
        value >>= 7;
        i += 1;
    }
    buffer[i] = (value as u8) & 0x7F;
    (buffer, i + 1)
}

pub fn decode_varint(bytes: &[u8], cursor: &mut usize) -> u32 {
    let mut value = 0u32;
    let mut shift = 0;
    while *cursor < bytes.len() {
        let byte = bytes[*cursor];
        *cursor += 1;
        value |= ((byte & 0x7F) as u32) << shift;
        if byte & 0x80 == 0 {
            return value;
        }
        shift += 7;
    }
    value
}

pub fn encode_delta(ids: &[u32]) -> Vec<u8> {
    let mut encoded = Vec::new();
    let mut last = 0;
    for &id in ids {
        let delta = id - last;
        let (buf, len) = encode_varint(delta);
        encoded.extend_from_slice(&buf[..len]);
        last = id;
    }
    encoded
}

pub fn decode_delta(bytes: &[u8], count: usize) -> Vec<u32> {
    let mut ids = Vec::with_capacity(count);
    let mut last = 0;
    let mut cursor = 0;
    for _ in 0..count {
        let delta = decode_varint(bytes, &mut cursor);
        let id = last + delta;
        ids.push(id);
        last = id;
    }
    ids
}

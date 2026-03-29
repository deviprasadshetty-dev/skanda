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
    if ids.is_empty() { return vec![]; }
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
        if cursor >= bytes.len() { break; }
        let delta = decode_varint(bytes, &mut cursor);
        let id = last + delta;
        ids.push(id);
        last = id;
    }
    ids
}

/// Encodes a mapping of BlockID -> [BytePositions]
pub fn encode_inverted_entry(block_positions: &[(u32, Vec<u32>)]) -> Vec<u8> {
    let mut encoded = Vec::new();
    let mut last_block_id = 0;
    
    for (block_id, positions) in block_positions {
        // 1. Encode BlockID Delta
        let block_delta = block_id - last_block_id;
        let (buf, len) = encode_varint(block_delta);
        encoded.extend_from_slice(&buf[..len]);
        last_block_id = *block_id;
        
        // 2. Encode Position Count
        let (buf, len) = encode_varint(positions.len() as u32);
        encoded.extend_from_slice(&buf[..len]);
        
        // 3. Encode Position Deltas
        let mut last_pos = 0;
        for &pos in positions {
            let pos_delta = pos - last_pos;
            let (buf, len) = encode_varint(pos_delta);
            encoded.extend_from_slice(&buf[..len]);
            last_pos = pos;
        }
    }
    encoded
}

pub fn decode_inverted_entry(bytes: &[u8]) -> Vec<(u32, Vec<u32>)> {
    let mut results = Vec::new();
    let mut cursor = 0;
    let mut last_block_id = 0;
    
    while cursor < bytes.len() {
        // 1. Decode BlockID
        let block_delta = decode_varint(bytes, &mut cursor);
        let block_id = last_block_id + block_delta;
        last_block_id = block_id;
        
        // 2. Decode Position Count
        let pos_count = decode_varint(bytes, &mut cursor) as usize;
        
        // 3. Decode Positions
        let mut positions = Vec::with_capacity(pos_count);
        let mut last_pos = 0;
        for _ in 0..pos_count {
            let pos_delta = decode_varint(bytes, &mut cursor);
            let pos = last_pos + pos_delta;
            positions.push(pos);
            last_pos = pos;
        }
        
        results.push((block_id, positions));
    }
    results
}

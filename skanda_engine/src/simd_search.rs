#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

pub fn find_substring(haystack: &str, needle: &str) -> Option<usize> {
    let h_bytes = haystack.as_bytes();
    let n_bytes = needle.as_bytes();

    if n_bytes.is_empty() { return Some(0); }
    if n_bytes.len() > h_bytes.len() { return None; }
    if n_bytes.len() == 1 {
        return h_bytes.iter().position(|&b| b == n_bytes[0]);
    }

    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("avx2") {
            if let Some(pos) = unsafe { find_avx2(h_bytes, n_bytes) } {
                return Some(pos);
            }
        } else if is_x86_feature_detected!("sse2") {
            if let Some(pos) = unsafe { find_sse2(h_bytes, n_bytes) } {
                return Some(pos);
            }
        }
    }

    haystack.find(needle)
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn find_avx2(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    let n_len = needle.len();
    let first = _mm256_set1_epi8(needle[0] as i8);
    let last = _mm256_set1_epi8(needle[n_len - 1] as i8);
    
    let mut i = 0;
    while i + 32 + n_len <= haystack.len() {
        let block_first = _mm256_loadu_si256(haystack.as_ptr().add(i) as *const __m256i);
        let block_last = _mm256_loadu_si256(haystack.as_ptr().add(i + n_len - 1) as *const __m256i);

        let eq_first = _mm256_cmpeq_epi8(block_first, first);
        let eq_last = _mm256_cmpeq_epi8(block_last, last);
        
        let mut mask = _mm256_movemask_epi8(_mm256_and_si256(eq_first, eq_last)) as u32;

        while mask != 0 {
            let offset = mask.trailing_zeros() as usize;
            if &haystack[i + offset..i + offset + n_len] == needle {
                return Some(i + offset);
            }
            mask &= mask - 1;
        }
        i += 32;
    }
    None
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse2")]
unsafe fn find_sse2(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    let n_len = needle.len();
    let first = _mm_set1_epi8(needle[0] as i8);
    let last = _mm_set1_epi8(needle[n_len - 1] as i8);
    
    let mut i = 0;
    while i + 16 + n_len <= haystack.len() {
        let block_first = _mm_loadu_si128(haystack.as_ptr().add(i) as *const __m128i);
        let block_last = _mm_loadu_si128(haystack.as_ptr().add(i + n_len - 1) as *const __m128i);

        let eq_first = _mm_cmpeq_epi8(block_first, first);
        let eq_last = _mm_cmpeq_epi8(block_last, last);
        
        let mut mask = _mm_movemask_epi8(_mm_and_si128(eq_first, eq_last)) as u32;

        while mask != 0 {
            let offset = mask.trailing_zeros() as usize;
            if &haystack[i + offset..i + offset + n_len] == needle {
                return Some(i + offset);
            }
            mask &= mask - 1;
        }
        i += 16;
    }
    None
}

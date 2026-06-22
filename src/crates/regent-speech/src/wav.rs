//! Minimal PCM→WAV encoder. Remote ASR APIs (OpenAI-compatible
//! `/audio/transcriptions`) want an uploadable audio file, and the canonical
//! [`AudioBuffer`](regent_kernel::AudioBuffer) is raw `i16` PCM — so the edge
//! wraps it in a 44-byte WAV header before upload. Pure and testable; no codecs
//! (lossy formats are produced by ffmpeg in the decode/convert path, not here).

use regent_kernel::AudioBuffer;

const HEADER_LEN: usize = 44;
const BITS_PER_SAMPLE: u16 = 16;

/// Encode `audio` as a 16-bit PCM WAV (little-endian).
#[must_use]
pub fn encode(audio: &AudioBuffer) -> Vec<u8> {
    let channels = audio.channels.max(1);
    let bytes_per_sample = u32::from(BITS_PER_SAMPLE / 8);
    let data_len = (audio.samples.len() * 2) as u32;
    let byte_rate = audio.sample_rate * u32::from(channels) * bytes_per_sample;
    let block_align = channels * (BITS_PER_SAMPLE / 8);

    let mut out = Vec::with_capacity(HEADER_LEN + audio.samples.len() * 2);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    // fmt chunk
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // PCM fmt chunk size
    out.extend_from_slice(&1u16.to_le_bytes()); // audio format = PCM
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&audio.sample_rate.to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&block_align.to_le_bytes());
    out.extend_from_slice(&BITS_PER_SAMPLE.to_le_bytes());
    // data chunk
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for sample in &audio.samples {
        out.extend_from_slice(&sample.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_a_well_formed_header_and_payload() {
        let audio = AudioBuffer::new(vec![0, 256, -256, 32_767], 16_000, 1);
        let wav = encode(&audio);

        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[12..16], b"fmt ");
        assert_eq!(&wav[36..40], b"data");
        // 44-byte header + 4 samples * 2 bytes.
        assert_eq!(wav.len(), HEADER_LEN + 8);

        // Sample rate (offset 24) and channels (offset 22) read back.
        let rate = u32::from_le_bytes(wav[24..28].try_into().unwrap());
        let channels = u16::from_le_bytes(wav[22..24].try_into().unwrap());
        assert_eq!(rate, 16_000);
        assert_eq!(channels, 1);
        // First sample little-endian.
        assert_eq!(&wav[44..46], &0i16.to_le_bytes());
        assert_eq!(&wav[46..48], &256i16.to_le_bytes());
    }

    #[test]
    fn riff_size_field_matches_total_minus_eight() {
        let audio = AudioBuffer::new(vec![1; 10], 8_000, 2);
        let wav = encode(&audio);
        let riff_size = u32::from_le_bytes(wav[4..8].try_into().unwrap());
        assert_eq!(riff_size as usize, wav.len() - 8);
    }
}

//! Minimal RIFF/WAV reader for ASR input: 16-bit PCM mono (exactly what the
//! call UI encodes). Walks chunks properly, so extra chunks (LIST, fact…)
//! don't break parsing. Anything else errors with a clear message.

/// Parse a 16-bit PCM mono WAV into `(sample_rate, f32 samples in [-1, 1])`.
pub fn parse_pcm16_mono(bytes: &[u8]) -> Result<(u32, Vec<f32>), String> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(
            "not a WAV file (voice notes in OGG/Opus aren't supported yet — \
                    use WAV input)"
                .into(),
        );
    }
    let u16le = |o: usize| u16::from_le_bytes([bytes[o], bytes[o + 1]]);
    let u32le = |o: usize| u32::from_le_bytes([bytes[o], bytes[o + 1], bytes[o + 2], bytes[o + 3]]);
    let mut pos = 12;
    let mut format: Option<(u16, u16, u32, u16)> = None; // (codec, channels, rate, bits)
    let mut data: Option<&[u8]> = None;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let size = u32le(pos + 4) as usize;
        let body = pos + 8;
        if body + size > bytes.len() {
            break; // truncated chunk — use what we have
        }
        match id {
            b"fmt " if size >= 16 => {
                format = Some((
                    u16le(body),
                    u16le(body + 2),
                    u32le(body + 4),
                    u16le(body + 14),
                ));
            }
            b"data" => data = Some(&bytes[body..body + size]),
            _ => {}
        }
        pos = body + size + (size & 1); // chunks are word-aligned
    }
    let Some((codec, channels, rate, bits)) = format else {
        return Err("WAV has no fmt chunk".into());
    };
    if codec != 1 || bits != 16 {
        return Err(format!(
            "expected 16-bit PCM WAV, got codec {codec} / {bits}-bit"
        ));
    }
    if channels != 1 {
        return Err(format!("expected mono WAV, got {channels} channels"));
    }
    let Some(data) = data else {
        return Err("WAV has no data chunk".into());
    };
    let samples = data
        .chunks_exact(2)
        .map(|c| f32::from(i16::from_le_bytes([c[0], c[1]])) / 32768.0)
        .collect();
    Ok((rate, samples))
}

#[cfg(test)]
mod tests {
    use super::*;
    use regent_kernel::AudioBuffer;

    #[test]
    fn round_trips_the_encoder_from_regent_speech() {
        let buf = AudioBuffer::new(vec![0, 16384, -16384, 32767], 16_000, 1);
        let bytes = regent_speech::wav::encode(&buf);
        let (rate, samples) = parse_pcm16_mono(&bytes).unwrap();
        assert_eq!(rate, 16_000);
        assert_eq!(samples.len(), 4);
        assert!((samples[1] - 0.5).abs() < 0.001);
        assert!((samples[2] + 0.5).abs() < 0.001);
    }

    #[test]
    fn rejects_non_wav_and_wrong_shapes() {
        assert!(parse_pcm16_mono(b"OggS....").is_err());
        let stereo = AudioBuffer::new(vec![0, 0], 16_000, 2);
        assert!(parse_pcm16_mono(&regent_speech::wav::encode(&stereo)).is_err());
    }
}

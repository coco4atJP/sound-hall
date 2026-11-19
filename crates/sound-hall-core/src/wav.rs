use std::io::{self, Write};

fn checked_wav_sizes(
    sample_rate: u32,
    channels: u16,
    sample_count: usize,
    bytes_per_sample: u32,
) -> io::Result<(u32, u16, u32, u32)> {
    if sample_rate == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "WAV sample_rate must be non-zero",
        ));
    }
    if channels == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "WAV channels must be non-zero",
        ));
    }
    let block_align = channels
        .checked_mul(bytes_per_sample as u16)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "WAV block_align overflow"))?;
    let byte_rate = sample_rate
        .checked_mul(block_align as u32)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "WAV byte_rate overflow"))?;
    let data_bytes_usize = sample_count
        .checked_mul(bytes_per_sample as usize)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "WAV data size overflow"))?;
    let data_bytes = u32::try_from(data_bytes_usize)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "WAV exceeds 4 GiB RIFF limit"))?;
    let riff_size = 36u32
        .checked_add(data_bytes)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "WAV RIFF size overflow"))?;
    Ok((data_bytes, block_align, byte_rate, riff_size))
}

/// Writes interleaved audio samples (`[-1.0, 1.0]`) to a standard 16-bit PCM WAV stream.
/// When `normalize` is true, scales the buffer peak to `0.95` (-0.45 dBFS) without clipping.
pub fn write_wav_i16<W: Write>(
    writer: &mut W,
    sample_rate: u32,
    channels: u16,
    samples: &[f32],
    normalize: bool,
) -> io::Result<()> {
    let (data_bytes, block_align, byte_rate, riff_size) =
        checked_wav_sizes(sample_rate, channels, samples.len(), 2)?;

    writer.write_all(b"RIFF")?;
    writer.write_all(&riff_size.to_le_bytes())?;
    writer.write_all(b"WAVE")?;

    // fmt chunk
    writer.write_all(b"fmt ")?;
    writer.write_all(&16u32.to_le_bytes())?;
    writer.write_all(&1u16.to_le_bytes())?; // PCM
    writer.write_all(&channels.to_le_bytes())?;
    writer.write_all(&sample_rate.to_le_bytes())?;
    writer.write_all(&byte_rate.to_le_bytes())?;
    writer.write_all(&block_align.to_le_bytes())?;
    writer.write_all(&16u16.to_le_bytes())?; // bits per sample

    // data chunk
    writer.write_all(b"data")?;
    writer.write_all(&data_bytes.to_le_bytes())?;

    let scale = if normalize {
        let peak = samples
            .iter()
            .copied()
            .filter(|s| s.is_finite())
            .fold(0.0_f32, |m, s| m.max(s.abs()));
        if peak > 1e-6 {
            0.95 / peak
        } else {
            1.0
        }
    } else {
        1.0
    };

    let mut chunk_buf = Vec::with_capacity(4096);
    for &sample in samples {
        let s = if sample.is_finite() {
            (sample * scale).clamp(-1.0, 1.0)
        } else {
            0.0
        };
        let pcm = (s * 32767.0).round() as i16;
        chunk_buf.extend_from_slice(&pcm.to_le_bytes());
        if chunk_buf.len() >= 4096 {
            writer.write_all(&chunk_buf)?;
            chunk_buf.clear();
        }
    }
    if !chunk_buf.is_empty() {
        writer.write_all(&chunk_buf)?;
    }
    Ok(())
}

/// Writes interleaved audio samples (`[-1.0, 1.0]`) to a 32-bit IEEE float WAV stream.
pub fn write_wav_f32<W: Write>(
    writer: &mut W,
    sample_rate: u32,
    channels: u16,
    samples: &[f32],
) -> io::Result<()> {
    let (data_bytes, block_align, byte_rate, riff_size) =
        checked_wav_sizes(sample_rate, channels, samples.len(), 4)?;

    writer.write_all(b"RIFF")?;
    writer.write_all(&riff_size.to_le_bytes())?;
    writer.write_all(b"WAVE")?;

    writer.write_all(b"fmt ")?;
    writer.write_all(&16u32.to_le_bytes())?;
    writer.write_all(&3u16.to_le_bytes())?; // IEEE float
    writer.write_all(&channels.to_le_bytes())?;
    writer.write_all(&sample_rate.to_le_bytes())?;
    writer.write_all(&byte_rate.to_le_bytes())?;
    writer.write_all(&block_align.to_le_bytes())?;
    writer.write_all(&32u16.to_le_bytes())?;

    writer.write_all(b"data")?;
    writer.write_all(&data_bytes.to_le_bytes())?;

    let mut chunk_buf = Vec::with_capacity(4096);
    for &sample in samples {
        let s = if sample.is_finite() {
            sample.clamp(-1.0, 1.0)
        } else {
            0.0
        };
        chunk_buf.extend_from_slice(&s.to_le_bytes());
        if chunk_buf.len() >= 4096 {
            writer.write_all(&chunk_buf)?;
            chunk_buf.clear();
        }
    }
    if !chunk_buf.is_empty() {
        writer.write_all(&chunk_buf)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_header_matches_pcm16_spec() {
        let mut buf = Vec::new();
        let samples = vec![0.0_f32, 0.5, -0.5, 1.0];
        write_wav_i16(&mut buf, 44100, 2, &samples, false).unwrap();
        assert_eq!(&buf[0..4], b"RIFF");
        assert_eq!(&buf[8..12], b"WAVE");
        assert_eq!(&buf[12..16], b"fmt ");
        assert_eq!(&buf[36..40], b"data");
        assert_eq!(buf.len(), 44 + samples.len() * 2);
    }
}

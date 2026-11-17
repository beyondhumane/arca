//! Arca: choosing a RAR 5 filter for each region of a streaming run.
//!
//! The streaming writer reads a member in runs and compresses runs in
//! parallel, so a filter has to be decided before the run is handed out, on
//! the thread that reads, and before the run becomes the history of the next
//! one: the decoder's window holds filtered bytes, so the history the encoder
//! matches against has to hold them too. That rules out trial encodes. Each
//! region is judged from its own byte statistics instead, the way WinRAR
//! cuts executables and media into 64 KiB pieces and filters each piece on
//! its own:
//!
//! - x86 code is recognised by `E8`/`E9` instructions whose operand is a near
//!   relative address, and filtered when turning those operands into absolute
//!   targets makes more of them repeat.
//! - 32-bit ARM code is recognised by its always-executed `BL` words. ARM64
//!   has no RAR 5 filter.
//! - Everything else that is not text is a candidate for delta. A sample ranks
//!   the widths by the order-0 entropy of their differences, and the best two
//!   are priced over the whole region as the planes RAR 5 delta really
//!   writes, each plane on its own, against the raw bytes priced both at
//!   order 0 and by a rough LZ estimate: data the match finder already
//!   handles is left alone, since splitting it into planes splits its
//!   repeats too.
//!
//! A region is filtered only when the estimate says it shrinks, so text,
//! source, logs and compressed data come out byte for byte as they did
//! without a policy.

use super::*;

/// The most input one region covers, and so one filter record. Runs start on
/// a multiple of this within the member, which keeps a 4-byte ARM word
/// aligned to the member.
const REGION: usize = 64 * 1024;

/// Smaller regions are left alone: the record costs more than the little a
/// filter can save there.
const MIN_REGION: usize = 4 * 1024;

/// The delta widths tried on every region: samples per frame of 8- and
/// 16-bit audio, pixels of 24- and 32-bit images, and the record sizes of
/// tables. A header's frame or pixel size is tried as well.
const DELTA_CHANNELS: [usize; 10] = [1, 2, 3, 4, 6, 8, 12, 16, 24, 32];

/// Bytes of each region the delta screen reads, as slices spread over it.
const SAMPLE_SLICE: usize = 4 * 1024;
const SAMPLE_SLICES: usize = 4;

/// The sample screen goes on only when the best width's differences cost at
/// most this fraction of the raw bytes, in order-0 bits. Loose on purpose:
/// mixing the planes in one histogram understates what delta saves.
const DELTA_SCREEN: (u64, u64) = (98, 100);

/// The planes have to cost at most this fraction of the raw region.
const DELTA_CONFIRM: (u64, u64) = (95, 100);

/// Near `E8` calls per region, scaled to its length: below this many per
/// 64 KiB a region is not code, or not enough of it to pay for a record.
const X86_MIN_NEAR_CALLS: usize = 64;

/// Repeats the filter has to add per 64 KiB before it is worth applying.
const X86_MIN_GAIN: usize = 16;

/// Always-executed `BL` words per 64 KiB, and the share of all words that are
/// always executed, below which a region is not ARM code.
const ARM_MIN_CALLS: usize = 128;
const ARM_MIN_ALWAYS_PERCENT: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Code {
    /// No header said; the statistics decide.
    Unknown,
    X86,
    Arm,
    /// A header named an architecture RAR 5 has no filter for.
    Neither,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Hint {
    code: Code,
    channels: Option<usize>,
}

const NO_HINT: Hint = Hint {
    code: Code::Unknown,
    channels: None,
};

/// Decides, applies and records the filters for one run of a member.
///
/// `data` is the run, transformed in place. `file_offset` is where it starts
/// in the member, which the x86 and ARM filters encode positions against.
/// `block_ends` are the run's block boundaries, as offsets into `data`; no
/// region crosses one, so each block can carry its own records. The records
/// come back in order, with offsets into `data`.
pub(crate) fn detect_filters<B: Budget>(
    data: &mut [u8],
    file_offset: u64,
    block_ends: impl IntoIterator<Item = usize>,
    allowance: &B,
) -> Result<Buffer<EncodeFilter, B>> {
    let hint = if file_offset == 0 {
        sniff(data)
    } else {
        NO_HINT
    };
    let mut records = Buffer::new(allowance);
    let mut block_start = 0usize;
    for block_end in block_ends {
        if block_end <= block_start || block_end > data.len() {
            return Err(Error::InvalidData("RAR 5 streaming block range is invalid"));
        }
        let mut start = block_start;
        while start < block_end {
            let end = (start + REGION).min(block_end);
            let region_offset = file_offset.wrapping_add(start as u64);
            if let Some((filter_type, channels)) = choose(&data[start..end], region_offset, hint) {
                apply(
                    &mut data[start..end],
                    filter_type,
                    channels,
                    region_offset,
                    allowance,
                )?;
                records
                    .push(EncodeFilter {
                        offset: start,
                        length: end - start,
                        filter_type,
                        channels,
                    })
                    .map_err(Into::into)?;
            }
            start = end;
        }
        block_start = block_end;
    }
    Ok(records)
}

fn apply<B: Budget>(
    region: &mut [u8],
    filter_type: FilterType,
    channels: usize,
    file_offset: u64,
    allowance: &B,
) -> Result<()> {
    // The decoder hands the filter its start as a 32-bit file offset, so the
    // encoder wraps the same way for members past 4 GiB.
    let file_offset = file_offset as u32;
    match filter_type {
        FilterType::E8 => e8e9_encode(region, file_offset, false),
        FilterType::E8E9 => e8e9_encode(region, file_offset, true),
        FilterType::Arm => arm_encode(region, file_offset),
        FilterType::Delta => {
            let transformed = filters::delta_encode_with_allowance(
                region,
                channels,
                rar50_delta_messages(),
                allowance,
            )?;
            region.copy_from_slice(&transformed);
        }
    }
    Ok(())
}

fn choose(region: &[u8], file_offset: u64, hint: Hint) -> Option<(FilterType, usize)> {
    if region.len() < MIN_REGION || is_text(region) {
        return None;
    }
    if matches!(hint.code, Code::Unknown | Code::X86) {
        if let Some(filter) = x86_filter(region, file_offset) {
            return Some((filter, 0));
        }
    }
    if matches!(hint.code, Code::Unknown | Code::Arm) && is_arm_code(region, file_offset) {
        return Some((FilterType::Arm, 0));
    }
    delta_channels(region, hint.channels).map(|channels| (FilterType::Delta, channels))
}

/// What a header at the start of a member says about it: the architecture of
/// an executable, or the frame or pixel size of audio and images.
fn sniff(data: &[u8]) -> Hint {
    let u16_at = |at: usize| -> Option<u16> {
        data.get(at..at + 2)
            .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]))
    };
    let u32_at = |at: usize| -> Option<u32> {
        data.get(at..at + 4)
            .map(|bytes| u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    };
    let code = |code| Hint {
        code,
        channels: None,
    };
    let channels = |channels: usize| Hint {
        code: Code::Unknown,
        channels: (1..=MAX_DELTA_CHANNELS)
            .contains(&channels)
            .then_some(channels),
    };
    if data.starts_with(b"MZ") {
        let Some(pe) = u32_at(0x3c).and_then(|at| usize::try_from(at).ok()) else {
            return NO_HINT;
        };
        if data.get(pe..pe.saturating_add(4)) != Some(b"PE\0\0") {
            return NO_HINT;
        }
        return match u16_at(pe + 4) {
            Some(0x014c | 0x8664) => code(Code::X86),
            Some(0x01c0) => code(Code::Arm),
            Some(0x01c2 | 0x01c4 | 0xaa64) => code(Code::Neither),
            _ => NO_HINT,
        };
    }
    if data.starts_with(b"\x7fELF") {
        let machine = match data.get(5) {
            Some(1) => u16_at(18),
            Some(2) => u16_at(18).map(u16::swap_bytes),
            _ => None,
        };
        return match machine {
            Some(3 | 62) => code(Code::X86),
            Some(40) => code(Code::Arm),
            Some(_) => code(Code::Neither),
            None => NO_HINT,
        };
    }
    if matches!(u32_at(0), Some(0xfeed_face | 0xfeed_facf)) {
        return match u32_at(4) {
            Some(0x0000_0007 | 0x0100_0007) => code(Code::X86),
            Some(0x0000_000c) => code(Code::Arm),
            Some(_) => code(Code::Neither),
            None => NO_HINT,
        };
    }
    if data.starts_with(b"RIFF") && data.get(8..12) == Some(b"WAVE") {
        let mut at = 12usize;
        while let (Some(id), Some(size)) = (data.get(at..at + 4), u32_at(at + 4)) {
            if id == b"fmt " {
                return match (u16_at(at + 8), u16_at(at + 20)) {
                    (Some(1 | 3 | 0xfffe), Some(align)) => channels(usize::from(align)),
                    _ => NO_HINT,
                };
            }
            let Some(next) = usize::try_from(size)
                .ok()
                .and_then(|size| at.checked_add(8)?.checked_add(size + (size & 1)))
            else {
                break;
            };
            at = next;
        }
        return NO_HINT;
    }
    if data.starts_with(b"BM") && u32_at(14).is_some_and(|size| size >= 40) {
        return match (u16_at(28), u32_at(30)) {
            (Some(24), Some(0)) => channels(3),
            (Some(32), Some(0 | 3)) => channels(4),
            _ => NO_HINT,
        };
    }
    NO_HINT
}

/// Text never gains from a filter, and checking for it first keeps the
/// screens off the most common data there is.
fn is_text(region: &[u8]) -> bool {
    let sample = &region[..region.len().min(SAMPLE_SLICE)];
    let text = sample
        .iter()
        .filter(|&&byte| matches!(byte, b'\t' | b'\n' | b'\r' | 0x20..=0x7e))
        .count();
    text * 10 >= sample.len() * 9
}

/// The x86 filter for this region, if it looks like code and the filter makes
/// more call targets repeat. E9 jumps are converted too when they gain on
/// their own.
fn x86_filter(region: &[u8], file_offset: u64) -> Option<FilterType> {
    if region.len() <= 4 {
        return None;
    }
    // Direct-mapped memories of the operands seen, for calls and jumps apart,
    // as relative operands and as the absolute targets the filter would
    // write. The tag bit keeps an empty slot from matching a zero operand.
    let mut absolute = [[0u64; 1024]; 2];
    let mut relative = [[0u64; 1024]; 2];
    let slot = |value: u32| (value.wrapping_mul(0x9e37_79b1) >> 22) as usize;
    let mut near_calls = 0usize;
    let mut gain = [0isize; 2];
    let limit = region.len() - 4;
    let mut pos = 0usize;
    while let Some(at) = crate::codec::fast::next_x86_opcode(region, pos, limit, 0xfe) {
        let operand = u32::from_le_bytes([
            region[at + 1],
            region[at + 2],
            region[at + 3],
            region[at + 4],
        ]);
        if matches!(operand >> 24, 0x00 | 0xff) {
            let kind = usize::from(region[at] == 0xe9);
            near_calls += usize::from(kind == 0);
            let target = operand.wrapping_add(file_offset.wrapping_add(at as u64 + 1) as u32);
            let tagged_target = u64::from(target) | 1 << 32;
            let tagged_operand = u64::from(operand) | 1 << 32;
            gain[kind] += isize::from(absolute[kind][slot(target)] == tagged_target);
            gain[kind] -= isize::from(relative[kind][slot(operand)] == tagged_operand);
            absolute[kind][slot(target)] = tagged_target;
            relative[kind][slot(operand)] = tagged_operand;
        }
        pos = at + 5;
    }
    let scale = |per_region: usize| (per_region * region.len()).div_ceil(REGION);
    if near_calls < scale(X86_MIN_NEAR_CALLS) || gain[0] < scale(X86_MIN_GAIN) as isize {
        return None;
    }
    Some(
        if gain[1] >= scale(X86_MIN_GAIN) as isize && gain[1] * 2 >= gain[0] {
            FilterType::E8E9
        } else {
            FilterType::E8
        },
    )
}

fn is_arm_code(region: &[u8], file_offset: u64) -> bool {
    let skip = (4 - (file_offset % 4) as usize) % 4;
    let (words, _) = region.get(skip..).unwrap_or_default().as_chunks::<4>();
    let total = words.len();
    let (mut calls, mut always) = (0usize, 0usize);
    for word in words {
        always += usize::from(word[3] >> 4 == 0xe);
        calls += usize::from(word[3] == 0xeb);
    }
    calls * REGION >= ARM_MIN_CALLS * region.len()
        && always * 100 >= total * ARM_MIN_ALWAYS_PERCENT
        && total > 0
}

/// The delta width for this region, if one pays.
fn delta_channels(region: &[u8], hinted: Option<usize>) -> Option<usize> {
    let slices = sample_slices(region);
    let raw = slices_cost(&slices, 0);
    let mut ranked = [(u64::MAX, 0usize); 2];
    for channels in DELTA_CHANNELS {
        let cost = slices_cost(&slices, channels);
        if cost < ranked[1].0 {
            ranked[1] = (cost, channels);
            if ranked[1].0 < ranked[0].0 {
                ranked.swap(0, 1);
            }
        }
    }
    if hinted.is_none()
        && ranked[0].0.saturating_mul(DELTA_SCREEN.1) > raw.saturating_mul(DELTA_SCREEN.0)
    {
        return None;
    }
    let sampled: u64 = slices.iter().map(|slice| slice.len() as u64).sum();
    let estimated: u64 = slices.iter().map(|slice| lz_estimate(slice)).sum();
    let plain = planes_cost(region, 1, false).min(estimated * region.len() as u64 / sampled);
    let (cost, channels) = [ranked[0].1, ranked[1].1, hinted.unwrap_or(0)]
        .into_iter()
        .filter(|&channels| channels != 0)
        .map(|channels| (planes_cost(region, channels, true), channels))
        .min()?;
    (cost * DELTA_CONFIRM.1 <= plain * DELTA_CONFIRM.0).then_some(channels)
}

/// The order-0 cost, in thousandths of a bit, of `region` cut into `channels`
/// planes the way RAR 5 delta cuts it, each plane priced on its own and
/// corrected for the bins a short plane leaves empty. With `delta` the planes
/// hold the differences the filter writes.
fn planes_cost(region: &[u8], channels: usize, delta: bool) -> u64 {
    let mut total = 0u64;
    for channel in 0..channels {
        let mut counts = [0u32; 256];
        let mut previous = 0u8;
        for &byte in region.iter().skip(channel).step_by(channels) {
            let value = if delta {
                previous.wrapping_sub(byte)
            } else {
                byte
            };
            previous = byte;
            counts[usize::from(value)] += 1;
        }
        let samples: u32 = counts.iter().sum();
        if samples == 0 {
            continue;
        }
        let used = counts.iter().filter(|&&count| count != 0).count() as f64;
        let correction = (used - 1.0) / (2.0 * std::f64::consts::LN_2) * 1000.0;
        total += entropy_millibits(&counts, samples) + correction as u64;
    }
    total
}

fn sample_slices(region: &[u8]) -> [&[u8]; SAMPLE_SLICES] {
    let mut slices: [&[u8]; SAMPLE_SLICES] = [&[]; SAMPLE_SLICES];
    if region.len() <= SAMPLE_SLICE * SAMPLE_SLICES {
        for (slot, chunk) in slices.iter_mut().zip(region.chunks(SAMPLE_SLICE)) {
            *slot = chunk;
        }
        return slices;
    }
    let step = (region.len() - SAMPLE_SLICE) / (SAMPLE_SLICES - 1);
    for (index, slot) in slices.iter_mut().enumerate() {
        *slot = &region[index * step..index * step + SAMPLE_SLICE];
    }
    slices
}

/// Order-0 cost, in thousandths of a bit, of the slices' bytes, or of their
/// differences `channels` apart.
fn slices_cost(slices: &[&[u8]], channels: usize) -> u64 {
    let mut counts = [0u32; 256];
    for slice in slices {
        if channels == 0 {
            for &byte in *slice {
                counts[usize::from(byte)] += 1;
            }
        } else if let Some(later) = slice.get(channels..) {
            for (&byte, &earlier) in later.iter().zip(slice.iter()) {
                counts[usize::from(earlier.wrapping_sub(byte))] += 1;
            }
        }
    }
    let total: u32 = counts.iter().sum();
    // A width that leaves no differences to count says nothing in its favour.
    if total == 0 {
        return u64::MAX / 2;
    }
    let bytes: usize = slices.iter().map(|slice| slice.len()).sum();
    entropy_millibits(&counts, total) * bytes as u64 / u64::from(total)
}

fn entropy_millibits(counts: &[u32; 256], total: u32) -> u64 {
    let total_f = f64::from(total);
    let bits: f64 = counts
        .iter()
        .filter(|&&count| count != 0)
        .map(|&count| {
            let count = f64::from(count);
            count * (total_f / count).log2()
        })
        .sum();
    (bits * 1000.0) as u64
}

/// A rough size, in thousandths of a bit, of `data` under a greedy LZ with a
/// single-entry hash: order-0 bits for the literals and a flat price per
/// match. Only ever compared with itself.
fn lz_estimate(data: &[u8]) -> u64 {
    const MATCH_MILLIBITS: u64 = 24_000;
    let mut table = [0u32; 4096];
    let mut counts = [0u32; 256];
    let mut matches = 0u64;
    let mut pos = 0usize;
    while pos + 4 <= data.len() {
        let key = u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]);
        let slot = (key.wrapping_mul(0x9e37_79b1) >> 20) as usize;
        let candidate = table[slot] as usize;
        table[slot] = pos as u32 + 1;
        if candidate != 0 && data[candidate - 1..candidate + 3] == data[pos..pos + 4] {
            let from = candidate - 1;
            let mut length = 4;
            while pos + length < data.len()
                && length < 258
                && data[from + length] == data[pos + length]
            {
                length += 1;
            }
            matches += 1;
            pos += length;
        } else {
            counts[usize::from(data[pos])] += 1;
            pos += 1;
        }
    }
    for &byte in &data[pos..] {
        counts[usize::from(byte)] += 1;
    }
    let total: u32 = counts.iter().sum();
    let literals = if total == 0 {
        0
    } else {
        entropy_millibits(&counts, total)
    };
    literals + matches * MATCH_MILLIBITS
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rar50::{ArchiveEntry, FilterPolicy, Rar50Writer, WriterOptions};
    use crate::{ArchiveReadOptions, ArchiveReader, ArchiveVersion, EntrySource, FeatureSet};

    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }

        fn below(&mut self, n: usize) -> usize {
            (self.next() % n as u64) as usize
        }
    }

    /// Code-like bytes: calls to a few hundred functions between filler that
    /// holds no `E8` or `E9`, and jumps among them as well when asked.
    fn x86(size: usize, jumps: bool) -> Vec<u8> {
        let mut rng = Rng(0x5eed_0001);
        let functions: Vec<usize> = (0..300).map(|_| rng.below(size)).collect();
        let mut out = Vec::with_capacity(size + 8);
        while out.len() < size {
            for _ in 0..rng.below(6) {
                out.push([0x48, 0x89, 0x45, 0x8b, 0x55, 0xc3, 0x0f, 0x85][rng.below(8)]);
            }
            let opcode = if jumps && rng.below(2) == 0 {
                0xe9
            } else {
                0xe8
            };
            let target = functions[rng.below(functions.len())];
            let relative = target as i64 - (out.len() as i64 + 5);
            out.push(opcode);
            out.extend_from_slice(&(relative as i32).to_le_bytes());
        }
        out.truncate(size);
        out
    }

    /// 32-bit ARM: always-executed instructions with a `BL` to one of a few
    /// targets every few words.
    fn arm(size: usize) -> Vec<u8> {
        let mut rng = Rng(0x5eed_0002);
        let targets: Vec<usize> = (0..64).map(|_| rng.below(size / 4)).collect();
        let mut out = Vec::with_capacity(size);
        while out.len() + 4 <= size {
            let word = out.len() / 4;
            if rng.below(6) == 0 {
                let offset = (targets[rng.below(targets.len())] as i64 - word as i64) as u32;
                out.extend_from_slice(&(offset & 0x00ff_ffff | 0xeb00_0000).to_le_bytes());
            } else {
                let top: u32 = [0xe1, 0xe3, 0xe5, 0xe8, 0xe0, 0xe2][rng.below(6)];
                let low = rng.next() as u32 & 0x00ff_ffff;
                out.extend_from_slice(&(low | top << 24).to_le_bytes());
            }
        }
        out.resize(size, 0);
        out
    }

    /// Stereo 16-bit PCM, with a WAV header when asked.
    fn pcm(size: usize, header: bool) -> Vec<u8> {
        let mut rng = Rng(0x5eed_0003);
        let mut out = Vec::with_capacity(size);
        if header {
            out.extend_from_slice(b"RIFF\0\0\0\0WAVEfmt \x10\0\0\0\x01\0\x02\0");
            out.extend_from_slice(&44_100u32.to_le_bytes());
            out.extend_from_slice(&(44_100u32 * 4).to_le_bytes());
            out.extend_from_slice(&[4, 0, 16, 0]);
            out.extend_from_slice(b"data\0\0\0\0");
        }
        let mut t = 0f64;
        while out.len() + 4 <= size {
            for channel in 0..2 {
                let value = (t * (0.013 + channel as f64 * 0.002)).sin() * 9000.0
                    + rng.below(64) as f64
                    - 32.0;
                out.extend_from_slice(&(value as i16).to_le_bytes());
            }
            t += 1.0;
        }
        out.resize(size, 0);
        out
    }

    /// A 24-bit bitmap: smooth colour ramps with a little noise.
    fn bitmap(size: usize) -> Vec<u8> {
        let mut rng = Rng(0x5eed_0004);
        let mut out = b"BM\0\0\0\0\0\0\0\0\x36\0\0\0\x28\0\0\0".to_vec();
        out.extend_from_slice(&[0, 4, 0, 0, 0, 4, 0, 0, 1, 0, 24, 0, 0, 0, 0, 0]);
        out.resize(54, 0);
        let mut pixel = 0usize;
        while out.len() < size {
            let (x, y) = (pixel % 1024, pixel / 1024);
            let noise = rng.below(5) as u8;
            out.extend_from_slice(&[
                (x / 4) as u8 ^ noise,
                (y * 3) as u8 ^ noise,
                ((x + y) / 8) as u8 ^ noise,
            ]);
            pixel += 1;
        }
        out.truncate(size);
        out
    }

    /// An array of 12-byte records whose fields creep from one to the next.
    fn table(size: usize) -> Vec<u8> {
        let mut rng = Rng(0x5eed_0005);
        let mut out = Vec::with_capacity(size + 12);
        let (mut a, mut b) = (0x0040_1000u32, 7u32);
        while out.len() < size {
            a = a.wrapping_add(16 + rng.below(48) as u32);
            b = b.wrapping_add(rng.below(3) as u32);
            out.extend_from_slice(&a.to_le_bytes());
            out.extend_from_slice(&b.to_le_bytes());
            out.extend_from_slice(&(rng.below(4) as u32).to_le_bytes());
        }
        out.truncate(size);
        out
    }

    fn text(size: usize) -> Vec<u8> {
        let words = [
            "archive ", "filter ", "delta ", "the ", "of ", "stream\n", "block ",
        ];
        let mut rng = Rng(0x5eed_0006);
        let mut out = Vec::with_capacity(size + 8);
        while out.len() < size {
            out.extend_from_slice(words[rng.below(words.len())].as_bytes());
        }
        out.truncate(size);
        out
    }

    fn random(size: usize) -> Vec<u8> {
        let mut rng = Rng(0x5eed_0007);
        (0..size).map(|_| rng.next() as u8).collect()
    }

    fn detected(data: &[u8], block_ends: &[usize]) -> (Vec<u8>, Vec<EncodeFilter>) {
        let mut transformed = data.to_vec();
        let records = detect_filters(
            &mut transformed,
            0,
            block_ends.iter().copied(),
            &Allowance::default(),
        )
        .unwrap();
        (transformed, records.into_vec())
    }

    fn kinds(records: &[EncodeFilter]) -> Vec<(FilterType, usize)> {
        let mut kinds: Vec<_> = records
            .iter()
            .map(|record| (record.filter_type, record.channels))
            .collect();
        kinds.dedup();
        kinds
    }

    fn archive(data: &[u8], level: u8, policy: FilterPolicy) -> Vec<u8> {
        Rar50Writer::new(
            WriterOptions::new(ArchiveVersion::Rar50, FeatureSet::store_only())
                .with_compression_level(level),
        )
        .entry(ArchiveEntry::new(
            b"member".to_vec(),
            EntrySource::from_bytes(data.to_vec()),
        ))
        .filter_policy(policy)
        .finish()
        .unwrap()
    }

    /// A filtered member is decoded whole, which the test build otherwise
    /// allows only for a kilobyte.
    fn read_back(bytes: &[u8]) -> crate::Result<Vec<u8>> {
        let options = || ArchiveReadOptions::default().with_rar50_buffered_decode_limit(64 << 20);
        let archive = ArchiveReader::read_with_options(bytes, options())?;
        Ok(archive
            .read_member_at_with_options(0, options())?
            .unwrap_or_default())
    }

    const DETECT: FilterPolicy = FilterPolicy::Detect {
        max_member: u64::MAX,
    };

    #[test]
    fn each_kind_of_data_gets_the_filter_made_for_it() {
        let size = 256 * 1024;
        for (data, expected) in [
            (x86(size, false), (FilterType::E8, 0)),
            (x86(size, true), (FilterType::E8E9, 0)),
            (arm(size), (FilterType::Arm, 0)),
            (pcm(size, true), (FilterType::Delta, 4)),
            (pcm(size, false), (FilterType::Delta, 4)),
            (bitmap(size), (FilterType::Delta, 3)),
            (table(size), (FilterType::Delta, 12)),
        ] {
            let (transformed, records) = detected(&data, &[size]);
            assert_eq!(kinds(&records), [expected], "{expected:?}");
            assert_eq!(records.len(), size / REGION);
            assert_ne!(transformed, data);
            let mut at = 0;
            for record in &records {
                assert_eq!(record.offset, at);
                assert!(record.length <= REGION);
                at += record.length;
            }
            assert_eq!(at, size);
        }
    }

    #[test]
    fn text_compressed_data_and_short_regions_are_left_alone() {
        for data in [
            text(300 * 1024),
            random(300 * 1024),
            x86(MIN_REGION - 1, false),
        ] {
            let len = data.len();
            let (transformed, records) = detected(&data, &[len]);
            assert!(records.is_empty());
            assert_eq!(transformed, data);
        }
    }

    #[test]
    fn regions_follow_block_boundaries() {
        let data = pcm(300 * 1024, false);
        let ends = [70_000, 70_000 + 4_000, 300 * 1024];
        let (_, records) = detected(&data, &ends);
        assert!(!records.is_empty());
        for record in &records {
            let block = ends.iter().position(|&end| record.offset < end).unwrap();
            let start = if block == 0 { 0 } else { ends[block - 1] };
            assert!(record.offset >= start);
            assert!(record.offset + record.length <= ends[block]);
        }
        // The 4,000-byte block is below the smallest region worth a record.
        assert!(records.iter().all(|record| record.offset != 70_000));
        for ends in [&[0][..], &[10, 5], &[data.len() + 1]] {
            let mut copy = data.clone();
            assert!(
                detect_filters(&mut copy, 0, ends.iter().copied(), &Allowance::default()).is_err()
            );
        }
    }

    #[test]
    fn headers_name_the_architecture_and_the_frame_size() {
        let mut pe = vec![0u8; 0x200];
        pe[..2].copy_from_slice(b"MZ");
        pe[0x3c] = 0x80;
        pe[0x80..0x84].copy_from_slice(b"PE\0\0");
        let mut elf = b"\x7fELF\x02\x01\x01".to_vec();
        elf.resize(64, 0);
        let mut macho = 0xfeed_facfu32.to_le_bytes().to_vec();
        macho.resize(32, 0);
        for (machine, code) in [
            (0x8664u16, Code::X86),
            (0x014c, Code::X86),
            (0x01c0, Code::Arm),
            (0xaa64, Code::Neither),
        ] {
            pe[0x84..0x86].copy_from_slice(&machine.to_le_bytes());
            assert_eq!(sniff(&pe).code, code, "PE {machine:#x}");
        }
        for (machine, code) in [(62u16, Code::X86), (40, Code::Arm), (183, Code::Neither)] {
            elf[18..20].copy_from_slice(&machine.to_le_bytes());
            assert_eq!(sniff(&elf).code, code, "ELF {machine}");
        }
        for (cpu, code) in [(0x0100_0007u32, Code::X86), (0x0100_000c, Code::Neither)] {
            macho[4..8].copy_from_slice(&cpu.to_le_bytes());
            assert_eq!(sniff(&macho).code, code, "Mach-O {cpu:#x}");
        }
        assert_eq!(sniff(&pcm(4096, true)).channels, Some(4));
        assert_eq!(sniff(&bitmap(4096)).channels, Some(3));
        assert_eq!(sniff(&text(4096)), NO_HINT);
        for header in [pe, elf, macho, pcm(64, true), bitmap(64)] {
            for len in 0..header.len() {
                let _ = sniff(&header[..len]);
                let mut corrupt = header[..len].to_vec();
                for byte in corrupt.iter_mut().skip(2) {
                    *byte = 0xff;
                }
                let _ = sniff(&corrupt);
            }
        }
    }

    #[test]
    fn filtered_members_round_trip_through_the_writer_and_reader() {
        // Past one run at level 1, so later runs match against filtered history.
        let size = 2 * 1024 * 1024 + 12_345;
        for data in [
            x86(size, false),
            x86(size, true),
            arm(size),
            pcm(size, true),
            bitmap(size),
            table(size),
        ] {
            for level in [1, 3] {
                let filtered = archive(&data, level, DETECT);
                let plain = archive(&data, level, FilterPolicy::None);
                assert!(filtered.len() < plain.len(), "level {level}");
                assert!(read_back(&filtered).unwrap() == data);
            }
        }
    }

    #[test]
    fn members_past_the_limit_and_solid_archives_are_not_filtered() {
        let data = pcm(300 * 1024, false);
        let len = data.len() as u64;
        let plain = archive(&data, 3, FilterPolicy::None);
        let over = archive(
            &data,
            3,
            FilterPolicy::Detect {
                max_member: len - 1,
            },
        );
        assert_eq!(over, plain);
        assert_ne!(
            archive(&data, 3, FilterPolicy::Detect { max_member: len }),
            plain
        );
        let mut features = FeatureSet::store_only();
        features.solid = true;
        let solid = |policy| {
            Rar50Writer::new(
                WriterOptions::new(ArchiveVersion::Rar50, features).with_compression_level(3),
            )
            .entry(ArchiveEntry::new(
                b"member".to_vec(),
                EntrySource::from_bytes(data.clone()),
            ))
            .filter_policy(policy)
            .finish()
            .unwrap()
        };
        assert_eq!(solid(DETECT), solid(FilterPolicy::None));
    }

    #[test]
    fn records_that_leave_their_block_are_refused() {
        let data = pcm(128 * 1024, false);
        let record = |offset, length| EncodeFilter {
            offset,
            length,
            filter_type: FilterType::Delta,
            channels: 4,
        };
        let blocks = [(64 * 1024, false), (128 * 1024, true)];
        for record in [
            record(60 * 1024, 8 * 1024),
            record(0, 0),
            record(0, MAX_FILTER_BLOCK_LENGTH + 1),
            record(128 * 1024, 16),
        ] {
            assert!(filtered_streaming_blocks_with_allowance(
                &data,
                &[],
                &blocks,
                0,
                EncodeOptions::new(8),
                &[record],
                None,
                &Allowance::default(),
            )
            .is_err());
        }
    }

    /// Every filter the format has, and every delta width, applied to a run
    /// that starts past a history at an offset no multiple of four, with
    /// records in two blocks.
    #[test]
    fn every_filter_and_width_decodes_after_history() {
        let allowance = Allowance::default();
        let options = EncodeOptions::new(8).with_max_match_distance(1 << 20);
        let history = pcm(100_003, false);
        let run = x86(200 * 1024, true);
        let blocks = [(128 * 1024, false), (run.len(), true)];
        let regions = [4096..68 * 1024, 130 * 1024..190 * 1024 + 7];
        let head = filtered_streaming_blocks_with_allowance(
            &history,
            &[],
            &[(history.len(), false)],
            0,
            options,
            &[],
            None,
            &allowance,
        )
        .unwrap();
        let mut expected = history.clone();
        expected.extend_from_slice(&run);
        let filters = [FilterType::E8, FilterType::E8E9, FilterType::Arm]
            .map(|filter_type| (filter_type, 0))
            .into_iter()
            .chain((1..=MAX_DELTA_CHANNELS).map(|channels| (FilterType::Delta, channels)));
        for (filter_type, channels) in filters {
            let mut data = run.clone();
            let mut records = Vec::new();
            for region in regions.clone() {
                let file_offset = (history.len() + region.start) as u64;
                apply(
                    &mut data[region.clone()],
                    filter_type,
                    channels,
                    file_offset,
                    &allowance,
                )
                .unwrap();
                records.push(EncodeFilter {
                    offset: region.start,
                    length: region.len(),
                    filter_type,
                    channels,
                });
            }
            assert_ne!(data, run, "{filter_type:?} {channels}");
            let tail = filtered_streaming_blocks_with_allowance(
                &data, &history, &blocks, 0, options, &records, None, &allowance,
            )
            .unwrap();
            let mut packed = Vec::new();
            for block in head.iter().chain(tail.iter()) {
                packed.extend_from_slice(block);
            }
            let decoded = crate::codec::rar50::decode_lz(&packed, 0, expected.len()).unwrap();
            assert!(decoded == expected, "{filter_type:?} {channels}");
        }
    }

    #[test]
    fn truncated_and_corrupted_filtered_members_never_panic() {
        let mut data = x86(160 * 1024, true);
        data.extend_from_slice(&pcm(160 * 1024, false));
        let bytes = archive(&data, 1, DETECT);
        assert!(bytes.len() < archive(&data, 1, FilterPolicy::None).len());
        let read = |bytes: &[u8]| {
            let _ = read_back(bytes);
        };
        for len in (0..bytes.len()).step_by(997) {
            read(&bytes[..len]);
        }
        for at in (0..bytes.len()).step_by(211) {
            let mut corrupt = bytes.clone();
            corrupt[at] ^= 0x5a;
            read(&corrupt);
        }
    }
}

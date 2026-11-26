use super::{BitReader, Error, Result};

fn bitwise_read(input: &[u8], cursor: &mut usize, count: u8) -> Result<u32> {
    let count = usize::from(count);
    let byte = *cursor / 8;
    let span = (*cursor % 8 + count).div_ceil(8);
    if span > input.len().saturating_sub(byte) {
        return Err(Error::NeedMoreInput);
    }
    let mut value = 0u32;
    for _ in 0..count {
        value = (value << 1) | u32::from((input[*cursor / 8] >> (7 - *cursor % 8)) & 1);
        *cursor += 1;
    }
    Ok(value)
}

#[test]
fn word_reads_match_independent_bits_at_every_alignment_and_physical_tail() {
    for input in [
        [0; 40],
        [0xff; 40],
        std::array::from_fn(|index| (index as u8).wrapping_mul(37).wrapping_add(11)),
    ] {
        for size in 0..=input.len() {
            for start in 0..=size * 8 + 16 {
                for count in 0..=40 {
                    let mut expected_cursor = start;
                    let expected = bitwise_read(&input[..size], &mut expected_cursor, count);
                    let mut actual = BitReader {
                        input: &input[..size],
                        bit_pos: start,
                    };
                    assert_eq!(
                        actual.read_bits(count),
                        expected,
                        "size={size}, start={start}, count={count}"
                    );
                    assert_eq!(actual.bit_pos, expected_cursor);
                }
            }
        }
    }
}

#[test]
fn word_reads_preserve_mixed_sequences_and_wider_low_word_results() {
    let input: Vec<_> = (0..4096usize)
        .map(|index| (index as u8).wrapping_mul(71).wrapping_add(29))
        .collect();
    let mut actual = BitReader::new(&input);
    let mut expected_cursor = 0;
    for count in [1, 4, 0, 8, 15, 26, 32, 33, 64, 127, 255]
        .into_iter()
        .cycle()
        .take(500)
    {
        let expected = bitwise_read(&input, &mut expected_cursor, count);
        assert_eq!(actual.read_bits(count), expected);
        assert_eq!(actual.bit_pos, expected_cursor);
    }
}

#[test]
fn word_read_refusal_retains_cursor_near_maximum_usize() {
    for start in [usize::MAX, usize::MAX - 7, usize::MAX - 15] {
        for count in [0, 1, 15, 32, 255] {
            let mut expected_cursor = start;
            let expected = bitwise_read(&[], &mut expected_cursor, count);
            let mut actual = BitReader {
                input: &[],
                bit_pos: start,
            };
            assert_eq!(actual.read_bits(count), expected);
            assert_eq!(actual.bit_pos, start);
        }
    }
}

#![expect(clippy::single_call_fn, reason = "readability")]
use crate::helpers::encode_hex;
use byteorder::ReadBytesExt;
use log::{Level, error, log_enabled, trace, warn};
use std::{
    ffi::OsStr,
    io::{self, BufRead, Cursor},
};

const LZ77_LOOKBACK_BITS: u8 = 12;
const LZ77_LENGTH_BITS: u8 = 4;
const LZ77_UNIT_SIZE: u8 = (LZ77_LOOKBACK_BITS + LZ77_LENGTH_BITS) / 8; // 2 bytes
const LZ77_WINDOW_SIZE: u16 = (1 << LZ77_LOOKBACK_BITS) - 1; // 4095 -- 12-bit lookback window
const LZ77_LE_MASK: [u8; LZ77_UNIT_SIZE as usize] = LZ77_WINDOW_SIZE.to_le_bytes(); // [0xFF, 0x0F]
const LZ77_MAX_LENGTH: u8 = (1 << LZ77_LENGTH_BITS) + LZ77_UNIT_SIZE; // 18 -- maximum allowed repeat length

pub fn deco_lz77_le<T: BufRead>(reader: &mut T) -> Result<(Vec<u8>, usize), io::Error> {
    trace!("Decompressing LZ77-LE data...");
    // Throw out the magic numer
    reader.read_exact(&mut [0; 2])?;
    // Now parse the length fields
    let mut current_field = [0; 4];
    reader.read_exact(&mut current_field)?;
    let expected_decompressed_size = u32::from_le_bytes(current_field).try_into().unwrap();
    reader.read_exact(&mut current_field)?;
    let compressed_size = u32::from_le_bytes(current_field).try_into().unwrap();
    let mut compressed_data = vec![0; compressed_size];
    trace!(
        "Compressed data size: {compressed_size}, expected decompressed size: {expected_decompressed_size}"
    );
    reader.read_exact(&mut compressed_data)?;
    let mut decompressed_data = Vec::with_capacity(expected_decompressed_size);
    let mut compressed_data_iter = compressed_data.into_iter();
    let mut mask = 0x01;
    let mut flag = reader.read_u8()?;
    // let mut flag_pos = blob_reader.position();
    while let Some(byte) = compressed_data_iter.next() {
        trace!(
            "Mask: 0x{mask:02x} Flag: 0x{flag:02x}, Result: 0x{:02x}",
            mask & flag
        );
        if mask & flag == 0 {
            decompressed_data.push(byte);
        } else {
            // Get the next compressed data byte in addition to the one we already have
            let next_byte = compressed_data_iter
                .next()
                .expect("Unexpected EOF while decompressing data.");
            // Combine the current and the next compressed bits into a u16, little endian, keeping only the 12 left bits.
            let mut lookback_bytes = [byte, next_byte];
            // Apply the mask to the lookback bytes
            for (a, b) in lookback_bytes.iter_mut().zip(LZ77_LE_MASK.iter().copied()) {
                *a &= b;
            }
            // Convert it to a u16 and add one.
            let lookback = u16::from_le_bytes(lookback_bytes) + 1;
            // Take the low order 4 bits of next_byte (little endian -- shift right), add the unit size, and 1 additional (we need to repeat at least SOMETHING, even if we only do it three times)
            let length = (next_byte >> LZ77_LENGTH_BITS) + LZ77_UNIT_SIZE + 1;

            if log_enabled!(Level::Trace) {
                trace!("next_byte {next_byte:02x}, length {length:02x}");
                trace!(
                    "Compressed: [{}], Lookback = x{lookback:02x} or [{}]",
                    encode_hex(&[byte, next_byte]),
                    encode_hex(&lookback.to_le_bytes())
                );
                trace!("{lookback} -> {length}");
            }

            // Where we start reading from in the decompressed data
            let skip = decompressed_data.len() - lookback as usize;
            // Read exactly `length` bytes. When we hit the end, we go back to where we skipped to and repeat until we fill exactly `length` bytes.
            let next_data = decompressed_data
                .iter()
                .skip(skip)
                .cycle()
                .take(length as usize)
                .copied()
                .collect::<Vec<_>>();
            // Take what was read above and append it to the already decompressed data
            if log_enabled!(Level::Trace) {
                trace!(
                    "Skip {skip} and extend deco with: {}",
                    encode_hex(&next_data)
                );
            }
            decompressed_data.extend(next_data);
            if decompressed_data.len() > expected_decompressed_size {
                error!(
                    "Expected decompressed size should be {expected_decompressed_size}, but got {}. Data corruption is likely.",
                    decompressed_data.len()
                );
                break;
            }
        }
        // Shift the mask so next time we read from the next flag bit (they're read from right to left)
        mask <<= 1;
        // When we have shifted all the way left, move on to the next flag byte
        if mask == 0 {
            flag = reader.read_u8()?;
            mask = 1;
        }

        // let soc_slice = &soc_data[..decompressed_data.len()];
        // if soc_slice != decompressed_data {
        //     for i in 0..decompressed_data.len() {
        //         let j = i.saturating_sub(8);
        //         if decompressed_data[i] != soc_slice[i] {
        //             panic!(
        //                 "Beginning at {j}\nGot: \n{}\nExpected:\n{}",
        //                 hexify(&decompressed_data[j..]),
        //                 hexify(&soc_slice[j..])
        //             )
        //         }
        //     }
        // }
    }

    trace!("Decompression finished...");
    Ok((decompressed_data, expected_decompressed_size))
}

pub fn compress_lz77_le(decompressed_data: &[u8]) -> Vec<u8> {
    trace!("Compressing data to LZ77-LE...");
    // The recompressed file should generally be smaller than this, but this gives us plenty of room to avoid allocations
    let mut compressed_data = Vec::with_capacity(decompressed_data.len());
    let mut flags = Vec::with_capacity(LZ77_WINDOW_SIZE as usize);

    let mut mask = 1;
    let mut flag = 0u8;
    // Seed the first byte
    #[expect(
        clippy::indexing_slicing,
        reason = "the file isn't going to be zero length"
    )]
    compressed_data.push(decompressed_data[0]);
    mask <<= 1;

    let mut deco_pos = 1;
    while deco_pos < decompressed_data.len() {
        // Determine the maximum allowable length so we don't overflow near the end
        #[expect(clippy::cast_possible_truncation, reason = "the lookback won't exceed 18 bytes, and this is a tight loop so a check would be expensive")]
        let max_length = (LZ77_MAX_LENGTH as usize).min(decompressed_data.len() - deco_pos) as u8;
        // trace!("max length {max_length}");
        // The longest continuous data match we find gets stored here
        let mut best_length = LZ77_UNIT_SIZE;
        // Pointer to the decompressed data (offset from start decompressed data) to read `best_length` bytes from.
        let mut best_lookback = 0;
        // The current lookback pointer we're searching from, within the search window
        let mut lookback = 0;
        while (deco_pos - lookback) > deco_pos.saturating_sub(LZ77_WINDOW_SIZE as usize) {
            // Go back one additional position within the search window
            lookback += 1;
            // The current length for this search position
            let mut length = 0;
            while length < max_length {
                // Check that each byte along both the search region and the current position to the length match
                // SAFETY:
                // The bound is checked in the loop condition
                let offset_byte = *unsafe {
                    decompressed_data.get_unchecked(deco_pos - lookback + length as usize)
                };
                // SAFETY:
                // The bound is checked in the loop condition
                let reference_byte =
                    *unsafe { decompressed_data.get_unchecked(deco_pos + length as usize) };
                // As soon as there is a mismatch, stop here
                if offset_byte != reference_byte {
                    break;
                }
                length += 1;
            }
            // If the length we found is higher than any previous findings, then record the length and lookback
            if length > best_length {
                best_length = length;
                best_lookback = lookback;
                // trace!("{best_lookback} -> {length}");
                // trace!("{best_lookback:04x} -> {length:02x}");
                // If we're already at the maximum allowable length, then just keep what we have and stop so we can advance the window
                if best_length == LZ77_MAX_LENGTH {
                    break;
                }
            }
        }

        if best_length <= LZ77_UNIT_SIZE {
            // trace!("Pushed {:04x?}", decompressed_data[deco_pos]);
            // SAFETY:
            // The bound is checked in the loop condition
            compressed_data.push(*unsafe { decompressed_data.get_unchecked(deco_pos) });
            deco_pos += 1;
        } else {
            // If we found a length greater than our smallest compression unit...
            // Flag the current offset as compressed data
            flag |= mask;
            // Convert the lookback we found to a little endian u16
            #[expect(clippy::cast_possible_truncation, reason = "the lookback won't exceed 18 bytes, and this is a tight loop so a check would be expensive")]
            let compressed_bytes = u16::to_le_bytes(best_lookback as u16 - 1);
            // Store the first byte verbatim
            // SAFETY:
            // The bound is checked in the loop condition
            compressed_data.push(*unsafe { compressed_bytes.get_unchecked(0) });
            // Merge the length indicator into the first nibble and store the whole byte.
            let length_bits = (best_length - LZ77_UNIT_SIZE - 1) << LZ77_LENGTH_BITS;
            // SAFETY:
            // The bound is checked in the loop condition
            compressed_data.push(*unsafe { compressed_bytes.get_unchecked(1) } | length_bits);
            // if log_enabled!(Level::Trace) {
            //     trace!("Length bits {:02x}", length_bits);
            //     trace!(
            //         "Pushed {}",
            //         encode_hex(&compressed_data[compressed_data.len() - 2..compressed_data.len()])
            //     );
            // }
            // We've compressed `best_length` bytes down to two, so advance the pointer by `best_length`
            deco_pos += best_length as usize;
        }

        // Advance the flag mask
        mask <<= 1;
        // If we're out of bits, store the current flag byte and start a new one
        if mask == 0 {
            mask = 1;
            flags.push(flag);
            flag = 0;
        }
    }
    flags.push(flag);

    let mut lz77_le_container = Vec::with_capacity(10 + compressed_data.len() + flags.len());
    lz77_le_container.extend(b"CM");
    #[expect(clippy::cast_possible_truncation, reason = "ps2 is a 32-bit platform, this value won't exceed that")]
    lz77_le_container.extend(u32::to_le_bytes(decompressed_data.len() as u32));
    #[expect(clippy::cast_possible_truncation, reason = "ps2 is a 32-bit platform, this value won't exceed that")]
    lz77_le_container.extend(u32::to_le_bytes(compressed_data.len() as u32));
    lz77_le_container.extend(compressed_data);
    lz77_le_container.extend(flags);
    trace!("Compression finished.");
    lz77_le_container
}

pub fn decompress(dat_name: &OsStr, file_number: i32, data: Vec<u8>) -> Result<Vec<u8>, io::Error> {
    let mut blob_reader = Cursor::new(data);
    let (decompressed_data, expected_size) = deco_lz77_le(&mut blob_reader)?;
    if decompressed_data.len() != expected_size {
        warn!(
            "Decompressed {}/{file_number:04} to {} bytes, but should be exactly: {expected_size} bytes. Data may be corrupted.",
            dat_name.to_string_lossy(),
            decompressed_data.len()
        );
    }
    #[expect(
        clippy::indexing_slicing,
        reason = "range is valid, this eliminates unnecessary padding"
    )]
    Ok(decompressed_data[0..decompressed_data.len()].to_vec())
}

// Use for unit testing
// let reconstituted_data = compress_lz77_le(&decompressed_data);

// for i in 0..reconstituted_data.len() {
//     let j = i.saturating_sub(8);
//     if reconstituted_data[i] != data[i] {
//         panic!(
//             "CM {stem_name} Beginning at {j}\nGot: \n{}\nExpected:\n{}",
//             encode_hex(&reconstituted_data[j..i + 8]),
//             encode_hex(&data[j..i + 8])
//         )
//     }
// }

// if reconstituted_data.len() != data.len() {
//     // Ensure the rest are just zeros
//     data.iter().enumerate().skip(reconstituted_data.len()+1).for_each(|(i, b)| {
//         if *b != 0 {
//         println!(
//             "Warning: CM {stem_name} Reconstituted data is {} bytes, original is {} bytes, last zero is at byte {}",
//             reconstituted_data.len(),
//             data.len(),
//             i-1
//         )

//         }
//     });
// }

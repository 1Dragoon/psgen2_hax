use crate::{
    events::{IndexMapWrapper, codec::parse_events, rebuild_event},
    helpers::{encode_hex, hex_edit_encode},
    lz77_le::{compress_lz77_le, decompress},
    save_binary_file,
    sggg_codec::{convert_to_png, png_to_sggg},
};
use alloc::{collections::BTreeMap, sync::Arc};
use core::time::Duration;
use log::{Level, debug, info, log_enabled, trace, warn};
use std::io::{self};
use std::{
    ffi::{OsStr, OsString},
    io::{Cursor, ErrorKind, Read, Seek},
    path::{Path, PathBuf},
};

enum HeaderType {
    Offsets,
    Sizes,
}

#[allow(clippy::panic_in_result_fn)]
pub fn read_subdat(
    data: &[u8],
    file_number: usize,
    base_file: &str,
) -> Result<Option<(usize, Vec<Vec<u8>>)>, io::Error> {
    let mut blob_reader = Cursor::new(data);
    let mut field = [0xFFu8; 4]; // Initialize a non-zero value to start the read loop
    let mut boundary_indicators = Vec::with_capacity(40);
    let mut header_type = HeaderType::Offsets;
    let mut last_field = 0;
    while field != [0u8; 4] {
        blob_reader.read_exact(&mut field)?;
        let field_value = usize::try_from(u32::from_le_bytes(field)).unwrap();
        // The any chunk offset or size exceeds the bounds of the data blob, then this isn't an sdat file.
        if field_value > data.len() {
            return Ok(None);
        }
        // A null value terminates the header
        if field_value != 0 {
            if field_value < last_field {
                header_type = HeaderType::Sizes;
            }
            boundary_indicators.push(field_value);
        }
        last_field = field_value;
    }
    // There are two types of these files:
    // - One type has a header that indicates the offset of each chunk
    // - The other type has a header that indicates the size of each chunk
    if boundary_indicators
        .last()
        .is_some_and(|offset| *offset > data.len())
    {
        return Ok(None);
    }
    while field == [0u8; 4] {
        blob_reader.read_exact(&mut field)?;
    }
    // Move the cursor just before the first non-zero field after the header
    blob_reader.seek_relative(-4)?;
    let first_chunk = if boundary_indicators.is_empty() {
        return Ok(None);
    } else {
        boundary_indicators[0]
    };
    let total_objects = boundary_indicators.len();
    let mut data_objects = Vec::with_capacity(total_objects);
    let current_position = usize::try_from(blob_reader.stream_position()?).unwrap();
    debug!("Current position {current_position:08x}, first_chunk {first_chunk:08x}");
    if current_position.is_multiple_of(0x10) && current_position == first_chunk {
        if matches!(header_type, HeaderType::Offsets) {
            info!("Extracting {total_objects} sub objects from {base_file}...");
            debug!("{base_file} chunk sizes are: {boundary_indicators:?}");
            let mut boundary_indicator_iter = boundary_indicators.into_iter().peekable();
            while let Some(current_boundary) = boundary_indicator_iter.next() {
                let current_position = usize::try_from(blob_reader.stream_position()?).unwrap();
                assert_eq!(current_boundary, current_position);
                let next_boundary = boundary_indicator_iter
                    .peek()
                    .copied()
                    .unwrap_or(data.len());
                let read_bytes = next_boundary - current_position;
                if read_bytes == 0 {
                    break;
                }
                debug!(
                    "Reading from {current_position} to {next_boundary} ({read_bytes} bytes) in {base_file} of size {}",
                    data.len()
                );
                let mut data_object = vec![0; read_bytes];
                blob_reader
                    .read_exact(&mut data_object)
                    .map_err(|err| format!("{err} in {base_file} while reading {read_bytes} bytes"))
                    .unwrap();
                data_objects.push(data_object);
            }
            debug!("Finished nested {base_file}");
            Ok(Some((first_chunk, data_objects)))
        } else {
            info!("Extracting {total_objects} sub objects from {base_file}...");
            println!("{base_file} chunk sizes are: {boundary_indicators:?}");
            let mut boundary_indicator_iter = boundary_indicators.into_iter().peekable();
            while let Some(current_size) = boundary_indicator_iter.next() {
                let current_position = usize::try_from(blob_reader.stream_position()?).unwrap();
                let next_size = boundary_indicator_iter
                    .peek()
                    .copied()
                    .unwrap_or(data.len()-current_position);
                let next_offset = next_size + current_position;
                println!(
                    "Reading from {current_position} to {next_offset} ({next_size} bytes) in {base_file} of size {}",
                    data.len()
                );
                let mut data_object = vec![0; next_size];
                blob_reader
                    .read_exact(&mut data_object)
                    .map_err(|err| format!("{err} in {base_file} while reading {next_size} bytes"))
                    .unwrap();
                data_objects.push(data_object);
            }
            debug!("Finished nested {base_file}");
            Ok(Some((first_chunk, data_objects)))
        }
    } else {
        Ok(None)
    }
}

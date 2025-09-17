mod helpers;
pub mod images;
pub mod lz77_le;
pub mod sjis_map;

use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions, create_dir_all};
use std::io::{BufRead, BufReader, BufWriter, Cursor, Read, Write};
use std::path::PathBuf;

use encoding_rs::SHIFT_JIS;
use file_type::FileType;

use crate::helpers::encode_hex;
use crate::images::{png_to_sggg, sggg_to_png};
use crate::lz77_le::{compress_lz77_le, deco_lz77_le};
use crate::sjis_map::{SHIS_DOUBLE_CHARS, SJIS_TO_UTF8_CHAR};

const DAT_BLOCK_SIZE: usize = 2048;

fn main() {
    let paths = fs::read_dir("d:/").unwrap();
    for path in paths {
        let dir_entry = path.unwrap();
        let path = dir_entry.path();
        // Ignore non-directories that aren't dat files.
        if path.is_dir()
            || path
                .extension()
                .is_some_and(|stem| !stem.to_string_lossy().to_lowercase().ends_with("dat"))
        {
            println!("Skipping '{}'", path.to_string_lossy());
            continue;
        }
        println!("{}", path.to_string_lossy());
        let dat_file = File::open(path).unwrap();
        let dat_file_size = dat_file.metadata().unwrap().len().try_into().unwrap();
        let mut dat_reader = BufReader::new(dat_file);

        unpack_dat(
            &mut dat_reader,
            dir_entry.file_name().as_os_str(),
            dat_file_size,
        );
    }
}

fn unpack_dat<T: BufRead>(dat_reader: &mut T, dat_name: &OsStr, dat_size: usize) {
    // DAT consists of a collection of 2048-byte blocks, akin to a filesystem but not quite. Block zero is the header.
    let total_blocks = dat_size / DAT_BLOCK_SIZE;
    let overflow_size = dat_size % DAT_BLOCK_SIZE; // Number of bytes that extend beyond the final block boundary
    if overflow_size > 0 {
        println!(
            "Warning: DAT file {} doesn't end evenly on a 2048-byte block boundary. {overflow_size} bytes after the final block will be truncated.",
            dat_name.to_string_lossy()
        )
    }
    let mut header = [0u8; DAT_BLOCK_SIZE];

    // Load the header into memory
    dat_reader.read_exact(&mut header).unwrap();
    // Create a cursor for parsing the header
    let mut header_reader = Cursor::new(header);

    // Header:
    //  - First 32-bit field is the total number of data blobs, each blob consisting of multiple blocks
    //  - Next is an array of 32-bit numbers, each pointing to block number offsets from the start of the file
    //  - The final offset points to EOF. Useful to indicate the final blob's end boundary.

    // A place in memory to put each 32-bit value we read
    let mut buf_u32 = [0u8; 4];

    // Determine the total number of blobs
    header_reader.read_exact(&mut buf_u32).unwrap();
    let blob_count: usize = u32::from_le_bytes(buf_u32).try_into().unwrap();

    // Create a place in memory where each block offset can be stored
    let mut block_offsets = Vec::with_capacity(blob_count + 1);

    // Store each block offset into memory
    for _ in 0..=blob_count {
        header_reader.read_exact(&mut buf_u32).unwrap();
        block_offsets.push(u32::from_le_bytes(buf_u32).try_into().unwrap())
    }

    println!("Extracting {} objects...", blob_count - 1);

    // Create a peakable iterator so that we can calculate each blob size as we read each offset
    let mut offsets_iter = block_offsets.into_iter().peekable();
    let mut file_number = 0;
    while let Some(offset) = offsets_iter.next() {
        // File stem name
        let stem_name = format!("{file_number:04}");
        // Determine the exact number of blocks to read for each blob
        let next_offset = offsets_iter.peek().unwrap_or(&total_blocks);
        let block_count = next_offset - offset;

        // Now read those blocks into a buffer
        let mut data = if block_count > 0 {
            vec![0; block_count * DAT_BLOCK_SIZE].into_boxed_slice()
        } else if overflow_size > 0 {
            vec![0; overflow_size].into_boxed_slice()
        } else {
            break;
        };
        dat_reader.read_exact(&mut data).unwrap();

        // Try to determine what kind of data this is. If we can't, just call it a .bin file.
        let file_type = FileType::from_bytes(&data);
        let mut extension = if let Some(extension) = file_type.extensions().iter().next().as_ref() {
            *extension
        } else {
            "bin"
        };

        if data[0..2] == *b"CM" {
            let mut blob_reader = Cursor::new(&data);
            let (decompressed_data, expected_size) = deco_lz77_le(&mut blob_reader);
            if decompressed_data.len() != expected_size {
                println!(
                    "Warning! Decompressed {}/{file_number:04} to {} bytes, but should be exactly: {expected_size} bytes. Data may be corrupted.",
                    dat_name.to_string_lossy(),
                    decompressed_data.len()
                );
            }

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

            data = decompressed_data.into_boxed_slice();
        }
        if data[0..4] == *b"SGGG" {
            // Reference? https://en.wikipedia.org/wiki/Segagaga
            // This file format seems most appropriate as a png rather than bmp given it has an alpha channel.
            // Harder to screw up, readily translates, and has an alpha channel
            let blob_reader = &mut Cursor::new(&data);
            let mut pngwriter = Cursor::new(vec![0; data.len()]);
            sggg_to_png(blob_reader, &mut pngwriter).unwrap();
            extension = "png";
            let pngdata = pngwriter.into_inner().into_boxed_slice();

            let reconstituted_data = png_to_sggg(&mut Cursor::new(&*pngdata)).unwrap();

            for i in 0..reconstituted_data.len() {
                let j = i.saturating_sub(8);
                if reconstituted_data[i] != data[i] {
                    panic!(
                        "SGGG {stem_name} Beginning at {j}\nGot: \n{}\nExpected:\n{}",
                        encode_hex(&reconstituted_data[j..i + 8]),
                        encode_hex(&data[j..i + 8])
                    )
                }
            }
            if reconstituted_data.len() != data.len() {
                println!(
                    "Warning: SGGG {stem_name} Reconstituted data is {} bytes, original is {} bytes",
                    reconstituted_data.len(),
                    data.len()
                )
            }

            data = pngdata;
        } else if dat_name.to_string_lossy().to_lowercase().contains("event") && file_number == 2 {
            let mut strings: Vec<Vec<String>> = Vec::new();
            let mut read_pos = 0;
            let mut current_string: Vec<String> = Vec::new();
            for read_pos in 0..data.len() {
                if SHIS_DOUBLE_CHARS.contains(&data[read_pos]) {
                    println!(
                        "{:02x} at {}: [0x{:02x}, 0x{:02x}]",
                        data[read_pos],
                        read_pos,
                        data[read_pos],
                        data[read_pos + 1]
                    );
                    if let Some(jap) = SJIS_TO_UTF8_CHAR.get(&[data[read_pos], data[read_pos + 1]])
                    {
                        println!("found one! {jap}");
                        current_string.push(jap.to_string());
                    }
                }
            }
            println!("{}", current_string.join(""));
            // let (res, _enc, errors) = SHIFT_JIS.decode(&data[0..484]);
            // println!("{}", res);
        }

        // Let's save the blob as a file, and here's how we name it
        let leaf_name = format!("{stem_name}.{}", extension);

        // let flags_size = reconstituted_data.len() - expected_size - 10;
        // println!(
        //     "Compressed length: {compressed_size}\nDecompressed length {decompressed_size}\nFlags length {}\nTotal length {}\nData offset {}",
        //     flags_size,
        //     reconstituted_data.len(),
        //     blob_reader.position()
        // );

        // let ft = FileType::from_bytes(&data);
        // println!("{ft:?}");

        let mut save_path = PathBuf::with_capacity(128);
        save_path.push("./");
        save_path.push(dat_name);

        // Create the directory if we haven't already
        create_dir_all(&save_path).unwrap();
        save_path.push(leaf_name);

        let f = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(save_path)
            .expect("Should be able to open file");
        let mut f = BufWriter::new(f);
        f.write_all(&data).expect("Should be able to write data");
        file_number += 1;
    }
}

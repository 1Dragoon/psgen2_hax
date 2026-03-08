use crate::{
    events::{IndexMapWrapper, codec::parse_events, rebuild_event},
    helpers::hex_edit_encode,
    lz77_le::{compress_lz77_le, decompress},
    save_binary_file,
    sggg_codec::{convert_to_png, png_to_sggg},
};
use alloc::collections::BTreeMap;
use core::time::Duration;
use log::{Level, debug, info, log_enabled, trace, warn};
use std::{
    ffi::{OsStr, OsString},
    io::{Cursor, ErrorKind},
    path::{Path, PathBuf},
};
use tokio::{
    fs::{self, create_dir_all},
    io::{self, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt},
    task::JoinHandle,
    time::sleep,
};

pub const DAT_BLOCK_SIZE: usize = 2048;

#[inline]
pub async fn parse_dat_header<T: AsyncBufReadExt + Unpin>(
    dat_reader: &mut T,
) -> Result<Vec<usize>, io::Error> {
    // Header:
    //  - First 32-bit field is the total number of data blobs, each blob consisting of multiple blocks
    //  - Next is an array of 32-bit numbers, each pointing to block number offsets from the start of the file
    //  - The final offset points to EOF. Useful to indicate the final blob's end boundary.
    let mut header = [0u8; DAT_BLOCK_SIZE];

    // Load the header into memory
    dat_reader.read_exact(&mut header).await?;
    // Create a cursor for parsing the header
    #[expect(
        clippy::absolute_paths,
        reason = "Would conflict with other function calls otherwise."
    )]
    let mut header_reader = std::io::Cursor::new(header);

    // A place in memory to put each 32-bit value we read
    let mut data_field = [0u8; 4];

    // Determine the total number of blobs
    header_reader.read_exact(&mut data_field).await?;
    let blob_count: usize = u32::from_le_bytes(data_field).try_into().unwrap();

    // Create a place in memory where each block offset can be stored
    let mut block_offsets = Vec::with_capacity(blob_count + 1);

    // Store each block offset into memory
    for _ in 0..=blob_count {
        header_reader.read_exact(&mut data_field).await?;
        block_offsets.push(u32::from_le_bytes(data_field).try_into().unwrap());
    }

    Ok(block_offsets)
}

#[inline]
pub async fn unpack_dat<T: AsyncBufReadExt + Unpin + Sync + Send, P: AsRef<Path> + Sync + Send>(
    dat_reader: &mut T,
    dat_name: &OsStr,
    dat_size: usize,
    out_dir: P,
    copy_images: bool,
) -> Result<(), io::Error> {
    // DAT consists of a collection of 2048-byte blocks, akin to a filesystem, but not quite. Block zero is the header.
    let total_blocks = dat_size / DAT_BLOCK_SIZE;
    let overflow_size = dat_size % DAT_BLOCK_SIZE; // Number of bytes that extend beyond the final block boundary
    if overflow_size > 0 {
        warn!(
            "DAT file {} doesn't end evenly on a 2048-byte block boundary. {overflow_size} bytes after the final block will be truncated.",
            dat_name.to_string_lossy()
        );
    }

    // Obtain the offsets for each game asset contained in the DAT file
    let block_offsets = parse_dat_header(dat_reader).await?;

    if log_enabled!(Level::Info) {
        info!("Extracting {} objects...", block_offsets.len());
    }

    // Create the directory if we haven't already
    let save_path = PathBuf::with_capacity(128).join(out_dir).join(dat_name);
    match create_dir_all(&save_path).await {
        Ok(()) => (),
        Err(err) => match err.kind() {
            ErrorKind::AlreadyExists => (),
            _ => return Err(err),
        },
    }

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
            vec![0; block_count * DAT_BLOCK_SIZE]
        } else if overflow_size > 0 {
            vec![0; overflow_size]
        } else {
            break;
        };
        dat_reader.read_exact(&mut data).await?;

        let mut extensions = Vec::with_capacity(3);

        if copy_images && data.iter().skip(10).take(4).copied().collect::<Vec<_>>() == b"SGGG" {
            // Just store the data file. No need to do anything else.
        } else {
            // Decompress the data payload first if necessary
            #[expect(clippy::indexing_slicing, reason = "more concise way to check magic")]
            if data[0..2] == *b"CM" {
                data = decompress(dat_name, file_number, data)?;
                extensions.push("lz77");
            }

            // SGGG files are a custom image format. We convert those to PNG.
            #[expect(clippy::indexing_slicing, reason = "more concise way to check magic")]
            if data[0..4] == *b"SGGG" {
                extensions.push("png");
                data = convert_to_png(data).unwrap();
            } else if dat_name.to_string_lossy().contains("EVENT") {
                // Only in the case of the event DAT file, we just assume non-SGGG are all event data.
                if log_enabled!(Level::Debug) {
                    debug!(
                        "\nEvent file: {file_number}, Size: {} ({:04x})",
                        data.len(),
                        data.len()
                    );
                }
                let mut event_reader = Cursor::new(&data);
                let (ordered_data, dialog_items) =
                    parse_events(&mut event_reader, u32::try_from(data.len()).unwrap())?;

                let dialog_file = save_path.clone().join(format!(
                    "{stem_name}.{}.eventdialog.toml",
                    extensions.join(".")
                ));
                // Save the event dialog separately, and only if it has any data
                if !dialog_items.is_empty() {
                    save_binary_file(
                        &dialog_file,
                        toml::to_string(&IndexMapWrapper(dialog_items))
                            .unwrap()
                            .as_bytes(),
                    )
                    .await?;
                }

                let events = IndexMapWrapper(ordered_data);
                extensions.push("eventdata");
                extensions.push("json");
                data = serde_json::to_string(&events).unwrap().into_bytes();
            }
        }

        let leaf_name = format!("{stem_name}.{}", extensions.join("."));
        let main_save_path = save_path.clone().join(leaf_name);

        save_binary_file(&main_save_path, &data).await?;
        file_number += 1;
    }
    Ok(())
}

// Packages each DAT component asset file concurently and in parallel
#[inline]
pub async fn pack_dat_components(path: &PathBuf) -> Result<BTreeMap<u16, Vec<u8>>, io::Error> {
    let mut dat_volume = BTreeMap::new();
    let mut tasks = Vec::with_capacity(384);
    let mut read_dir = fs::read_dir(path).await?;
    while let Some(subdir_entry) = read_dir.next_entry().await? {
        let component_file = subdir_entry.path();
        let component_file_str = component_file.to_string_lossy();
        if component_file_str.contains("eventdialog") || component_file_str.ends_with("bin") {
            continue;
        }
        tasks.push(tokio::spawn(async move {
            repackage_dat_component(component_file).await
        }));
    }
    while !tasks.is_empty() {
        for i in 0..tasks.len() {
            if tasks.get(i).is_some_and(JoinHandle::is_finished) {
                let task = tasks.remove(i);
                let (component_file, data) = task.await??;
                dat_volume.insert(component_file, data);
            }
        }
        sleep(Duration::from_millis(100)).await;
    }
    Ok(dat_volume)
}

// Construct a DAT file header
#[inline]
pub fn generate_dat_header(dat_components: &BTreeMap<u16, Vec<u8>>) -> Vec<u8> {
    let mut dat_header = Vec::with_capacity(DAT_BLOCK_SIZE);
    // Construct the header. First, total blocks indicator:
    dat_header.extend((u32::try_from(dat_components.len()).unwrap()).to_le_bytes());
    // Now each block offset:
    let mut current_block = 0;
    // Enumerate all of the component sizes, noting that the first will start at DAT_BLOCK_SIZE to account for the header itself
    let mut sizes = Vec::with_capacity(dat_components.len());
    sizes.push(DAT_BLOCK_SIZE);
    for data in dat_components.values() {
        sizes.push(data.len());
    }
    for size in sizes {
        // Calculate each block number when padding is considered
        if size % DAT_BLOCK_SIZE != 0 {
            current_block += 1;
        }
        current_block += size / DAT_BLOCK_SIZE;
        dat_header.extend(u32::try_from(current_block).unwrap().to_le_bytes());
    }
    // Pad the header data to the next block boundary
    dat_header.extend(vec![0u8; DAT_BLOCK_SIZE - dat_header.len()]);
    dat_header
}

fn pop_extension(path: &mut PathBuf) -> Option<OsString> {
    // Get the file stem (filename without extension)
    if let Some(stem) = path.file_stem() {
        // Get the extension
        if let Some(ext) = path.extension().map(OsStr::to_os_string) {
            // Remove the last extension by updating the path
            let new_stem = stem.to_owned();
            *path = path.with_file_name(new_stem);
            return Some(ext);
        }
    }
    None
}

// Repackage a single DAT component according to its extensions
#[inline]
#[expect(clippy::single_call_fn, reason = "Readability")]
pub async fn repackage_dat_component(
    mut component_file: PathBuf,
) -> Result<(u16, Vec<u8>), io::Error> {
    let mut data = Vec::with_capacity(usize::try_from(component_file.metadata()?.len()).unwrap());
    let file = fs::File::open(&*component_file).await?;
    let mut br = io::BufReader::new(file);
    br.read_to_end(&mut data).await?;
    br.flush().await?;
    while component_file.as_path().extension().is_some() {
        if log_enabled!(Level::Debug) {
            debug!(
                "Reconstructing block from {} starting with bytes {}",
                component_file.to_string_lossy(),
                hex_edit_encode(data.get(..7).unwrap_or_default())
            );
        }
        let extension = component_file
            .extension()
            .map(|e| e.to_string_lossy())
            .unwrap_or_default();
        match extension.as_ref() {
            "png" => {
                #[expect(
                    clippy::absolute_paths,
                    reason = "Would conflict with other function calls otherwise."
                )]
                let mut reader = std::io::Cursor::new(&data);
                data = png_to_sggg(&mut reader).unwrap();
            }
            "lz77" => {
                data = compress_lz77_le(&data);
                // println!("Recompressed data {}", encode_hex(&data));
            }
            "toml" | "json" | "eventdialog" | "bin" => (),
            "eventdata" => {
                let dialog_file_stem = component_file.file_stem().unwrap();
                let mut dialog_file_path = component_file.parent().unwrap().join(dialog_file_stem);
                dialog_file_path.add_extension("eventdialog");
                dialog_file_path.add_extension("toml");
                data = rebuild_event(
                    &data,
                    component_file.to_string_lossy().as_ref(),
                    dialog_file_path,
                )
                .unwrap();
                // println!("Rebuilt event: {}", encode_hex(&data));
            }
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    format!(
                        "For file {}: Unsupported file extension: {extension}",
                        component_file.canonicalize().unwrap().to_string_lossy()
                    ),
                ));
            }
        }
        pop_extension(&mut component_file);
    }
    let component_number = component_file
        .file_stem()
        .unwrap()
        .to_string_lossy()
        .parse::<u16>()
        .unwrap();
    Ok((component_number, data))
}

// Package a DAT file from component files in the specified directory
#[inline]
pub async fn pack_dat(path: &PathBuf, dest: &PathBuf) -> Result<(), io::Error> {
    // Convert, recompress and/or rebuild each game asset of the DAT file
    let dat_components = pack_dat_components(path).await?;
    // Generate the DAT header to start the DAT volume
    let mut dat_volume = generate_dat_header(&dat_components);
    // Now fill-in the data.
    for (_component_no, mut data) in dat_components {
        dat_volume.append(&mut data);
        // Pad to the next block boundary
        let next_boundary = DAT_BLOCK_SIZE - (dat_volume.len() % DAT_BLOCK_SIZE);
        if next_boundary != DAT_BLOCK_SIZE {
            trace!(
                "Current size: {} Next boundary: {next_boundary}",
                dat_volume.len()
            );
            dat_volume.extend(vec![0u8; next_boundary]);
        }
    }
    info!("Saving DAT to {}", dest.to_string_lossy());
    save_binary_file(dest, &dat_volume).await?;
    Ok(())
}

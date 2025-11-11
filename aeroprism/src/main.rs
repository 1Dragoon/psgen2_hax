#![allow(clippy::blanket_clippy_restriction_lints, reason = "not needed")]
#![warn(clippy::pedantic)]
#![warn(clippy::restriction)]
#![warn(clippy::nursery)]
#![allow(clippy::missing_docs_in_private_items, reason = "not needed")]
#![allow(clippy::implicit_return, reason = "not needed")]
#![allow(clippy::unseparated_literal_suffix, reason = "not needed")]
#![allow(clippy::else_if_without_else, reason = "not needed")]
#![allow(clippy::pub_with_shorthand, reason = "not needed")]
#![allow(clippy::field_scoped_visibility_modifiers, reason = "not needed")]
#![allow(clippy::similar_names, reason = "not needed")]
#![allow(clippy::little_endian_bytes, reason = "not needed")]
#![allow(clippy::unused_trait_names, reason = "not needed")]
#![allow(clippy::single_char_lifetime_names, reason = "not needed")]
#![allow(clippy::min_ident_chars, reason = "not needed")]
#![allow(clippy::mod_module_files, reason = "not needed")]
#![allow(clippy::non_ascii_literal, reason = "not needed")]
#![allow(clippy::default_numeric_fallback, reason = "not needed")]
#![allow(clippy::wildcard_enum_match_arm, reason = "not needed")]
#![allow(clippy::missing_trait_methods, reason = "not needed")]
#![allow(clippy::big_endian_bytes, reason = "not needed")]
#![allow(clippy::pattern_type_mismatch, reason = "not needed")]
#![allow(clippy::unreachable, reason = "not needed")]
#![allow(clippy::panic, reason = "will look it over later")]
#![allow(clippy::arithmetic_side_effects, reason = "will look it over later")]
#![allow(
    clippy::integer_division_remainder_used,
    reason = "will look it over later"
)]
#![allow(clippy::unwrap_used, reason = "will fix these later")]
#![allow(clippy::expect_used, reason = "will fix these later")]
#![allow(clippy::unwrap_in_result, reason = "will fix these later")]
#![allow(clippy::question_mark_used, reason = "will fix these later")]
#![allow(clippy::too_many_lines, reason = "will fix these later")]
#![allow(clippy::cognitive_complexity, reason = "will fix these later")]
#![allow(clippy::as_conversions, reason = "will fix these later")]
#![allow(clippy::integer_division, reason = "will fix these later")]
#![allow(clippy::single_call_fn, reason = "will fix these later")]
mod events;
mod helpers;
mod lz77_le;
mod sggg_codec;
extern crate alloc;
use crate::{
    events::{IndexMapWrapper, codec::parse_events, rebuild_event, save_dialog_strings},
    helpers::copy_dir_all,
    lz77_le::{compress_lz77_le, decompress},
    sggg_codec::{convert_to_png, png_to_sggg},
};
use alloc::collections::BTreeMap;
use clap::Parser;
use colog::basic_builder;
use core::time::Duration;
use env_logger::Target;
use log::{Level, LevelFilter, debug, info, log_enabled, trace, warn};
use shellexpand::path;
use soft_canonicalize::soft_canonicalize;
use std::{
    ffi::OsStr,
    io::Cursor,
    path::{Path, PathBuf},
    sync::OnceLock,
    time::Instant,
};
use tokio::{
    fs::{self, OpenOptions, create_dir_all},
    io::{self, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, BufWriter},
    runtime,
    task::JoinHandle,
    time::sleep,
};

const DAT_BLOCK_SIZE: usize = 2048;
static ENGRISH: OnceLock<bool> = OnceLock::new();

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    /// When unpacking data, copy over the images rather than decompressing/converting them. This saves time when rebuilding if you aren't going to modify any images.
    #[arg(short, long)]
    copy_images: bool,

    /// Whether the source files are from an English translation or a Japanese translation
    #[arg(short, long)]
    engrish: bool,

    /// The source directory to read from.
    /// When extracting to files, this is the path to the mounted ISO image.
    /// When repacking to an ISO, this is the path to the unpacked (that you can modify) files.
    in_path: PathBuf,

    /// The log level to use. The higher the level, the noisier the output.
    #[arg(short, long, default_value = "info")]
    log_level: LevelFilter,

    /// When extracting, this is where to put the extracted files
    /// When repacking to an ISO, this is where the repacked files go.
    #[arg(short, long, default_value = "./psg2_data")]
    out_path: PathBuf,

    /// Whether you're repacking to an ISO or extracting. Defaults to extracting.
    #[arg(short, long)]
    repack: bool,

    /// The number of threads to work with. If you're using an HDD, lowering this might help. Minimum value is 1, defaults to the number of CPU cores on your system.
    #[arg(short, long)]
    threads: Option<usize>,
}

fn main() {
    let cli = Cli::parse();
    let mut builder = runtime::Builder::new_multi_thread();
    if let Some(t) = cli.threads {
        builder.worker_threads(t);
    }
    builder.enable_all().build().unwrap().block_on(async {
        main_thread(cli).await.unwrap();
    });
}

async fn main_thread(cli: Cli) -> Result<(), io::Error> {
    // build_iso();
    // return Ok(());
    ENGRISH.set(cli.engrish).unwrap();
    let mut log_builder = basic_builder();
    log_builder.target(Target::Stdout);
    log_builder.filter(None, cli.log_level).init();
    debug!("Debug logging enabled!");
    trace!("Trace logging enabled!");
    let in_path = soft_canonicalize(path::full(&cli.in_path).unwrap()).unwrap();
    let out_path = soft_canonicalize(path::full(&cli.out_path).unwrap()).unwrap();

    if cli.repack {
        walk_build(in_path, out_path).await?;
    } else {
        walk_iso(&in_path, &out_path, cli.copy_images).await?;
    }
    Ok(())
}

#[expect(clippy::single_call_fn, reason = "Readability")]
async fn walk_build<P: AsRef<Path> + Sync + Send + Clone>(
    in_dir: P,
    out_dir: PathBuf,
) -> Result<(), io::Error> {
    fs::create_dir_all(&out_dir).await?;
    let now = Instant::now();
    let mut read_dir = fs::read_dir(in_dir).await.unwrap();
    let mut tasks = Vec::with_capacity(16);
    while let Some(dir_entry) = read_dir.next_entry().await.unwrap() {
        let od = out_dir.clone();
        tasks.push(tokio::spawn(async move {
            process_dir_entry(od, dir_entry).await
        }));
    }
    // let files = vec![
    //     "SYSTEM.CNF",
    //     "SLPM_625.53",
    //     "MAPDATA.DAT",
    //     "EVENT.DAT",
    //     "BTLDAT.DAT",
    //     "BTLSYS.DAT",
    //     "MODULE",
    //     "SOUND.DAT",
    //     "MONDAT.DAT",
    // ];
    // let mut builder = FileInput::empty();
    // for file in files {
    //     builder.append(hadris_iso::File { path: file, data: hadris_iso::FileData::Data(()) });
    // }
    // builder.append(file);
    while !tasks.is_empty() {
        for i in 0..tasks.len() {
            if tasks.get(i).is_some_and(JoinHandle::is_finished) {
                let task = tasks.remove(i);
                let path = task.await.unwrap().unwrap();
                info!("Completed {}", path.to_string_lossy());
            }
        }
        sleep(Duration::from_millis(100)).await;
    }
    #[expect(clippy::float_arithmetic, reason = "it's only for display")]
    let time = f64::from(u32::try_from(now.elapsed().as_millis()).unwrap()) / 1_000f64;
    info!("Total time: {time} sec",);
    Ok(())
}

#[expect(clippy::single_call_fn, reason = "Readability")]
async fn process_dir_entry(
    out_dir: PathBuf,
    dir_entry: fs::DirEntry,
) -> Result<PathBuf, io::Error> {
    let path = dir_entry.path();
    let dest = out_dir.join(path.file_name().unwrap());
    if path.is_dir() {
        info!("Processing '{}'", path.to_string_lossy());
        // Reconstruct DAT files
        if path.to_string_lossy().ends_with("DAT") {
            let mut dat_size = 0;
            let mut dat_components = BTreeMap::new();
            let mut tasks = Vec::with_capacity(384);
            let mut read_dir = fs::read_dir(&path).await.unwrap();
            while let Some(subdir_entry) = read_dir.next_entry().await.unwrap() {
                let component_file = subdir_entry.path();
                let component_file_str = component_file.to_string_lossy();
                if component_file_str.contains("eventdialog") || component_file_str.ends_with("bin")
                {
                    continue;
                }
                debug!(
                    "Reconstructing block from {}",
                    component_file.to_string_lossy()
                );
                tasks.push(tokio::spawn(
                    async move { reconstitute(component_file).await },
                ));
            }
            for task in tasks {
                let (component_file, data) = task.await.unwrap().unwrap();
                dat_components.insert(component_file, data.len());
                dat_size += data.len();
            }
            let mut dat_contents = Vec::with_capacity(dat_size);
            // Construct the header. First, total blocks ondicator:
            dat_contents.extend((u32::try_from(dat_components.len()).unwrap()).to_le_bytes());
            // Now each block offset:
            let mut current_block = 0;
            // Enumerate all of the component sizes, noting that the first will start at DAT_BLOCK_SIZE to account for the header itself
            let mut sizes = Vec::with_capacity(dat_components.len());
            sizes.push(DAT_BLOCK_SIZE);
            for size in dat_components.values() {
                sizes.push(*size);
            }
            for size in sizes {
                // Calculate each block number when padding is considered
                if size % DAT_BLOCK_SIZE != 0 {
                    current_block += 1;
                }
                current_block += size / DAT_BLOCK_SIZE;
                dat_contents.extend(u32::try_from(current_block).unwrap().to_le_bytes());
            }
            // Pad the header data to the next block boundary
            dat_contents.extend(vec![0u8; DAT_BLOCK_SIZE - dat_contents.len()]);
            // Header is finished, now put all of the files in
            for (component_file, _size) in dat_components {
                let handle = fs::File::open(component_file).await.unwrap();
                let mut br = BufReader::new(handle);
                br.read_to_end(&mut dat_contents).await.unwrap();
                br.flush().await.unwrap();
                // Pad to the next block boundary
                let next_boundary = DAT_BLOCK_SIZE - (dat_contents.len() % DAT_BLOCK_SIZE);
                if next_boundary != DAT_BLOCK_SIZE {
                    trace!(
                        "Current size: {} Next boundary: {next_boundary}",
                        dat_contents.len()
                    );
                    dat_contents.extend(vec![0u8; next_boundary]);
                }
            }
            // let dat_path = out_dir.as_ref().join(path.file_name().unwrap());
            info!("Saving DAT to {}", dest.to_string_lossy());
            let dat_component = OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .open(&dest)
                .await
                .unwrap();
            let mut bw = BufWriter::new(dat_component);
            bw.write_all(&dat_contents).await.unwrap();
            bw.flush().await.unwrap();
        } else {
            copy_dir_all(&path, &dest).await?;
        }
    } else if path
        .extension()
        .is_some_and(|stem| !stem.to_string_lossy().ends_with("DAT"))
    {
        info!(
            "Copying '{}' to '{}'",
            path.to_string_lossy(),
            dest.to_string_lossy()
        );
        if path != dest {
            #[cfg(target_os = "windows")]
            if dest.exists() {
                use fs::set_permissions;
                let mut perms = fs::metadata(&dest).await?.permissions();
                if perms.readonly() {
                    #[expect(
                        clippy::permissions_set_readonly_false,
                        reason = "lint is only relevant to non-windows systems"
                    )]
                    perms.set_readonly(false);
                    set_permissions(&dest, perms).await?;
                }
            }
            fs::copy(path, &dest).await.unwrap();
        }
    }
    Ok(dest)
}

#[expect(clippy::single_call_fn, reason = "Readability")]
async fn reconstitute(mut component_file: PathBuf) -> Result<(PathBuf, Vec<u8>), io::Error> {
    let mut data =
        Vec::with_capacity(usize::try_from(component_file.metadata().unwrap().len()).unwrap());
    let file = fs::File::open(&*component_file).await.unwrap();
    let mut br = io::BufReader::new(file);
    br.read_to_end(&mut data).await.unwrap();
    br.flush().await.unwrap();
    while component_file.as_path().extension().is_some() {
        let extension = component_file.extension().unwrap().to_string_lossy();
        // println!("{}", component_file.to_string_lossy());
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
        component_file.set_extension("");
    }
    component_file.add_extension("bin");
    let mut out_file = fs::File::create(&component_file).await.unwrap();
    out_file.write_all(&data).await.unwrap();
    out_file.flush().await.unwrap();
    Ok((component_file, data))
}

#[expect(clippy::single_call_fn, reason = "Readability")]
async fn walk_iso<P: AsRef<Path> + Send + Sync>(
    in_dir: P,
    out_dir: P,
    copy_images: bool,
) -> Result<(), io::Error> {
    fs::create_dir_all(&out_dir).await?;
    let mut read_dir = fs::read_dir(&in_dir).await.unwrap();
    while let Some(dir_entry) = read_dir.next_entry().await.unwrap() {
        let path = dir_entry.path();
        let dest = out_dir.as_ref().join(path.file_name().unwrap());
        // Simply copy non-directories that aren't dat files.
        if path.is_dir() {
            copy_dir_all(&path, &dest).await?;
            continue;
        } else if path
            .extension()
            .is_some_and(|stem| !stem.to_string_lossy().ends_with("DAT"))
        {
            info!(
                "Copying '{}' to '{}'",
                path.to_string_lossy(),
                dest.to_string_lossy()
            );
            #[cfg(target_os = "windows")]
            if dest.exists() {
                let mut perms = fs::metadata(&dest).await?.permissions();
                if perms.readonly() {
                    #[expect(
                        clippy::permissions_set_readonly_false,
                        reason = "lint is only relevant to non-windows systems"
                    )]
                    perms.set_readonly(false);
                    fs::set_permissions(&dest, perms).await?;
                }
            }
            fs::copy(path, dest).await?;
            continue;
        }
        info!("Processing '{}'", path.to_string_lossy());
        let dat_file = fs::File::open(path).await?;
        let dat_file_size = dat_file.metadata().await?.len().try_into().unwrap();
        let mut dat_reader = BufReader::new(dat_file);

        unpack_dat(
            &mut dat_reader,
            dir_entry.file_name().as_os_str(),
            dat_file_size,
            &out_dir,
            copy_images,
        )
        .await?;
    }
    Ok(())
}

#[expect(clippy::single_call_fn, reason = "Readability")]
async fn unpack_dat<T: AsyncBufReadExt + Unpin, P: AsRef<Path>>(
    dat_reader: &mut T,
    dat_name: &OsStr,
    dat_size: usize,
    out_dir: P,
    copy_images: bool,
) -> Result<(), io::Error> {
    // DAT consists of a collection of 2048-byte blocks, akin to a filesystem but not quite. Block zero is the header.
    let total_blocks = dat_size / DAT_BLOCK_SIZE;
    let overflow_size = dat_size % DAT_BLOCK_SIZE; // Number of bytes that extend beyond the final block boundary
    if overflow_size > 0 {
        warn!(
            "DAT file {} doesn't end evenly on a 2048-byte block boundary. {overflow_size} bytes after the final block will be truncated.",
            dat_name.to_string_lossy()
        );
    }
    let mut header = [0u8; DAT_BLOCK_SIZE];

    // Load the header into memory
    dat_reader.read_exact(&mut header).await.unwrap();
    // Create a cursor for parsing the header
    #[expect(
        clippy::absolute_paths,
        reason = "Would conflict with other function calls otherwise."
    )]
    let mut header_reader = std::io::Cursor::new(header);

    // Header:
    //  - First 32-bit field is the total number of data blobs, each blob consisting of multiple blocks
    //  - Next is an array of 32-bit numbers, each pointing to block number offsets from the start of the file
    //  - The final offset points to EOF. Useful to indicate the final blob's end boundary.

    // A place in memory to put each 32-bit value we read
    let mut buf_u32 = [0u8; 4];

    // Determine the total number of blobs
    header_reader.read_exact(&mut buf_u32).await.unwrap();
    let blob_count: usize = u32::from_le_bytes(buf_u32).try_into().unwrap();

    // Create a place in memory where each block offset can be stored
    let mut block_offsets = Vec::with_capacity(blob_count + 1);

    // Store each block offset into memory
    for _ in 0..=blob_count {
        header_reader.read_exact(&mut buf_u32).await.unwrap();
        block_offsets.push(u32::from_le_bytes(buf_u32).try_into().unwrap());
    }

    if log_enabled!(Level::Info) {
        info!("Extracting {} objects...", blob_count - 1);
    }

    // Create the directory if we haven't already
    let save_path = PathBuf::with_capacity(128).join(out_dir).join(dat_name);
    create_dir_all(&save_path).await?;

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
            #[expect(clippy::indexing_slicing, reason = "more concise way to check magic")]
            if data[0..2] == *b"CM" {
                data = decompress(dat_name, file_number, data)?;
                extensions.push("lz77");
            }
            #[expect(clippy::indexing_slicing, reason = "more concise way to check magic")]
            if data[0..4] == *b"SGGG" {
                extensions.push("png");
                data = convert_to_png(data)?;
            } else if dat_name.to_string_lossy().contains("EVENT") {
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
                    save_dialog_strings(&dialog_file, &IndexMapWrapper(dialog_items))?;
                }

                let events = IndexMapWrapper(ordered_data);
                extensions.push("eventdata");
                extensions.push("json");
                data = serde_json::to_string(&events).unwrap().into_bytes();
            }
        }

        let leaf_name = format!("{stem_name}.{}", extensions.join("."));
        let main_save_path = save_path.clone().join(leaf_name);

        let component_file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(main_save_path)
            .await
            .unwrap();
        let mut bw = BufWriter::new(component_file);
        bw.write_all(&data).await.unwrap();
        bw.flush().await.unwrap();
        file_number += 1;
    }
    Ok(())
}

// CD-ROM is in ISO 9660 format
// System id: PLAYSTATION
// Volume id:
// Volume set id:
// Publisher id:
// Data preparer id:
// Application id: PLAYSTATION
// Copyright File id: 3DAGES
// Abstract File id:
// Bibliographic File id:
// Volume set size is: 1
// Volume set sequence number is: 1
// Logical block size is: 2048
// Volume size is: 45918
// NO Joliet present
// NO Rock Ridge present

// 0, \x00
// 48, \x01
// 96,  SYSTEM.CNF;1
// 156, SLPM_625.53;1
// 216, MAPDATA.DAT;1
// 276, EVENT.DAT;1
// 334, BTLDAT.DAT;1
// 394, BTLSYS.DAT;1
// 454, MODULE
// 508, SOUND.DAT;1
// 566, MONDAT.DAT;1

// fn build_iso() {
//     use hadris_iso::{FileInput, FormatOptions, IsoImage, PartitionOptions, VolumeInternals};
//     use std::path::PathBuf;
//     // C:/Users/jjd/Documents/PCSX2/games/psg2english01.iso
//     let mut file = File::open("C:/Users/jjd/Documents/PCSX2/games/psgen2test.iso").unwrap();
//     // let mut br = BufReader::new(file);
//     // let mut iso = IsoImage::parse(&mut file).unwrap();
//     // let vd = iso.get_volume_descriptors().primary();
//     // println!("{vd:#?}");
//     // for (num, dir) in &iso.root_directory().entries().unwrap() {
//     //     println!("{num}, {}", dir.name.to_str());
//     // }
//     // let fila = FormatOptions::new();
//     // let foo = PartitionOptions::all();
//     let files = ["SYSTEM.CNF", "SLPM_625.53", "MAPDATA.DAT", "EVENT.DAT", "BTLDAT.DAT", "BTLSYS.DAT", "MODULE", "SOUND.DAT", "MONDAT.DAT"];
//     let mut builder = FileInput::empty();
//     for file in files {
//         builder.append(hadris_iso::File { path: file, data: hadris_iso::FileData::Data(()) });
//     }
//     builder.append(file);

//     for entry in fs::read_dir("C:/Users/jjd/code/psgen2_repack").unwrap() {}

//     let options = FormatOptions::new()
//         .with_files(FileInput::from_fs(PathBuf::from("path/to/files")).unwrap());
//     let file = IsoImage::format_file(PathBuf::from("path/to/image"), options).unwrap();
// }

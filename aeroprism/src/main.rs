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
#![allow(clippy::integer_division_remainder_used, reason = "not needed")]
#![allow(clippy::question_mark_used, reason = "not needed")]
#![allow(clippy::enum_variant_names, reason = "not needed")]
// Still in the prototyping stage of development
#![allow(clippy::arithmetic_side_effects, reason = "will revisit later")]
#![allow(clippy::unwrap_used, reason = "will fix these later")]
#![allow(clippy::expect_used, reason = "will fix these later")]
#![allow(clippy::unwrap_in_result, reason = "will fix these later")]
#![allow(clippy::too_many_lines, reason = "will fix these later")]
#![allow(clippy::cognitive_complexity, reason = "will fix these later")]
#![allow(clippy::as_conversions, reason = "will fix these later")]
#![allow(clippy::integer_division, reason = "will fix these later")]
#![allow(clippy::single_call_fn, reason = "will fix these later")]
mod dat_codec;
mod events;
mod helpers;
mod lz77_le;
mod sggg_codec;
mod slpm_patcher;
extern crate alloc;
#[cfg(target_os = "windows")]
use crate::helpers::unset_readonly;
use crate::{
    dat_codec::{pack_dat, unpack_dat},
    events::load_exec_patch,
    helpers::{copy_dir_all, save_binary_file},
    slpm_patcher::{ExecData, parse_end_credits, parse_enemies, parse_items, patch_end_credits},
};
use alloc::sync::Arc;
use clap::Parser;
use colog::basic_builder;
use core::time::Duration;
use env_logger::Target;
use log::{LevelFilter, debug, info, trace, warn};
use shellexpand::path;
use soft_canonicalize::soft_canonicalize;
use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
    time::Instant,
};
use tokio::{
    fs::{self, OpenOptions},
    io::{self, AsyncWriteExt, BufReader, BufWriter},
    runtime,
    task::JoinHandle,
    time::sleep,
};

static ENGRISH: OnceLock<bool> = OnceLock::new();

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    /// Whether the source files are from an English translation or a Japanese translation.
    #[arg(short, long)]
    engrish: bool,

    /// The source directory to read from.
    /// When extracting to files, this is the path to the mounted ISO image.
    /// When repacking to an ISO, this is the path to the unpacked (that you can modify) files.
    in_path: PathBuf,

    /// The log level to use. The higher the level, the noisier the output.
    #[arg(short, long, default_value = "info")]
    log_level: LevelFilter,

    /// When unpacking data, copy over the images rather than decompressing/converting them. This saves time when rebuilding if you aren't going to modify any images.
    #[arg(short, long)]
    no_image_processing: bool,

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

fn main() -> Result<(), io::Error> {
    let cli = Cli::parse();
    let mut builder = runtime::Builder::new_multi_thread();
    if let Some(t) = cli.threads {
        builder.worker_threads(t);
    }
    builder
        .enable_all()
        .build()
        .unwrap()
        .block_on(async { main_thread(cli).await })
}

async fn main_thread(cli: Cli) -> Result<(), io::Error> {
    // events::sjis_map::sjis_gen();
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
        walk_iso(&in_path, &out_path, cli.no_image_processing).await?;
    }
    Ok(())
}

#[expect(clippy::single_call_fn, reason = "Readability")]
async fn walk_build(in_dir: PathBuf, out_dir: PathBuf) -> Result<(), io::Error> {
    fs::create_dir_all(&out_dir).await?;
    let now = Instant::now();
    let mut read_dir = fs::read_dir(&in_dir).await.unwrap();
    let mut tasks = Vec::with_capacity(16);
    let sd = Arc::new((in_dir, out_dir));
    while let Some(dir_entry) = read_dir.next_entry().await.unwrap() {
        let dirs = Arc::clone(&sd);
        tasks.push(tokio::spawn(async move {
            process_dir_entry(dirs, dir_entry).await
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
                if let Some(path) = task.await?? {
                    info!("Completed {}", path.to_string_lossy());
                }
            }
        }
        sleep(Duration::from_millis(100)).await;
    }
    #[expect(clippy::float_arithmetic, reason = "it's only for display")]
    let time = f64::from(u32::try_from(now.elapsed().as_millis()).unwrap()) / 1_000f64;
    info!("Total time: {time} sec",);
    Ok(())
}

#[inline]
#[expect(clippy::single_call_fn, reason = "Readability")]
async fn process_dir_entry<P: AsRef<Path> + Send + Sync>(
    dirs: Arc<(P, P)>,
    dir_entry: fs::DirEntry,
) -> Result<Option<PathBuf>, io::Error> {
    let (in_dir, out_dir) = &*dirs;
    let path = dir_entry.path();
    let dest = out_dir.as_ref().join(path.file_name().unwrap());
    if path.is_dir() {
        let name_str = path.file_name().unwrap_or_default().to_string_lossy();
        // Reconstruct DAT files
        if name_str.ends_with("DAT") {
            info!("Processing '{}'", path.to_string_lossy());
            pack_dat(&path, &dest).await?;
        } else {
            // Ignore maths we don't need, e.g. git
            if name_str.starts_with('.') {
                return Ok(None);
            }
            info!(
                "Copying directory tree '{}' to '{}'",
                path.to_string_lossy(),
                dest.to_string_lossy()
            );
            copy_dir_all(&path, &dest).await?;
        }
    } else if path
        .extension()
        .is_some_and(|stem| !stem.to_string_lossy().ends_with("DAT"))
    {
        if dest
            .file_name()
            .is_some_and(|file_name| file_name.to_string_lossy().to_lowercase() == "exec_data.json")
        {
            return Ok(None);
        }
        info!(
            "Copying '{}' to '{}'",
            path.to_string_lossy(),
            dest.to_string_lossy()
        );
        if path != dest {
            #[cfg(target_os = "windows")]
            unset_readonly(&dest).await?;
            fs::copy(path, &dest).await?;
        }
        if dest
            .file_name()
            .is_some_and(|file_name| file_name.to_string_lossy().to_uppercase() == "SLPM_625.53")
            && let Some(exec_data_path) = find_json(in_dir)?
        {
            let ExecData {
                items: _a,
                enemies: _b,
                end_credits,
            } = load_exec_patch(exec_data_path)?;
            #[cfg(target_os = "windows")]
            unset_readonly(&dest).await?;
            let elf_binary = OpenOptions::new().write(true).open(&dest).await?;
            let mut bw = BufWriter::new(elf_binary);
            patch_end_credits(&mut bw, end_credits).await?;
            bw.flush().await?;
        }
    }
    Ok(Some(dest))
}

fn find_json<P: AsRef<Path> + Send + Sync>(
    path: P,
    // search_name: P,
) -> Result<Option<PathBuf>, io::Error> {
    for entry in path.as_ref().read_dir()? {
        let file = entry?.path();
        if file
            .to_string_lossy()
            .to_lowercase()
            .ends_with(&"exec_data.json")
        {
            return Ok(Some(file));
        }
    }
    Ok(None)
}

#[expect(clippy::single_call_fn, reason = "Readability")]
async fn walk_iso<P: AsRef<Path> + Send + Sync>(
    in_path: P,
    out_dir: P,
    copy_images: bool,
) -> Result<(), io::Error> {
    fs::create_dir_all(&out_dir).await?;
    let mut read_dir = fs::read_dir(&in_path).await?;
    while let Some(dir_entry) = read_dir.next_entry().await? {
        let path = dir_entry.path();
        let dest = out_dir.as_ref().join(path.file_name().unwrap());
        // Handle ELF binary
        if path.to_string_lossy().ends_with("SLPM_625.53") {
            generate_exec_data(&out_dir, &path).await?;
        }
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
            copy_file(&path, dest).await?;
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

async fn copy_file(path: &PathBuf, dest: PathBuf) -> Result<(), io::Error> {
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
    Ok(())
}

async fn generate_exec_data<P: AsRef<Path> + Send + Sync>(
    out_dir: &P,
    path: &PathBuf,
) -> Result<(), io::Error> {
    let elf_file = fs::File::open(path).await?;
    let mut elf_reader = BufReader::new(elf_file);
    let exec_data = ExecData {
        items: parse_items(&mut elf_reader).await,
        enemies: parse_enemies(&mut elf_reader).await,
        // strings: parse_map_strings(&mut elf_reader).await,
        end_credits: parse_end_credits(&mut elf_reader).await,
    };
    let exec_json = serde_json::to_string_pretty(&exec_data)
        .unwrap()
        .into_bytes();
    let save_path = PathBuf::with_capacity(128)
        .join(out_dir)
        .join("exec_data.json");
    save_binary_file(&save_path, exec_json).await;
    Ok(())
}

// More or less a unix-like "strings" function that is "PSG2 aware"
// async fn elf_bin_engrish_strings(elf_file_size: usize, mut elf_reader: BufReader<fs::File>) {
//     elf_reader.seek(SeekFrom::Start(0)).await.unwrap();
//     let mut elf_data = Vec::with_capacity(elf_file_size);
//     let mut i = elf_reader.stream_position().await.unwrap();
//     elf_reader.read_to_end(&mut elf_data).await.unwrap();
//     let mut elf_data_iter = elf_data.into_iter().peekable();
//     let mut strings = Vec::with_capacity(40);
//     let mut addr = 0;
//     while let Some(byte) = elf_data_iter.next() {
//         if strings.is_empty() {
//             addr = i; // + POINTER_OFFSET as u64;
//         }
//         i += 1;
//         if byte != 0
//             && let Ok(count) = parse_next_sjis(&mut elf_data_iter, &mut strings, byte)
//         {
//             i += u64::from(count - 1);
//         } else {
//             if strings.len() > 1 {
//                 // if log_enabled!(Level::Debug) {
//                 println!("0x{:02x}: {}", addr, strings.concat());
//                 // }
//             }
//             strings.clear();
//         }
//     }
// }

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

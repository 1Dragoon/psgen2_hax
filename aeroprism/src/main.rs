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
use crate::{
    dat_codec::{pack_dat, unpack_dat},
    helpers::{copy_dir_all, copy_file, save_binary_file},
    slpm_patcher::{generate_exec_data, patch_exec},
};
use alloc::sync::Arc;
use clap::Parser;
use colog::basic_builder;
use core::time::Duration;
use env_logger::Target;
use log::{Level, LevelFilter, debug, info, log_enabled, trace, warn};
use shellexpand::path;
use soft_canonicalize::soft_canonicalize;
use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
    time::Instant,
};
use tokio::{
    fs,
    io::{self, BufReader},
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
        repack(in_path, out_path).await?;
    } else {
        unpack(in_path, out_path, cli.no_image_processing).await?;
    }
    Ok(())
}

#[expect(clippy::single_call_fn, reason = "Readability")]
async fn repack(in_dir: PathBuf, out_dir: PathBuf) -> Result<(), io::Error> {
    let now = Instant::now();
    fs::create_dir_all(&out_dir).await?;
    let mut read_dir = fs::read_dir(&in_dir).await?;
    let mut tasks = Vec::with_capacity(16);
    let sd = Arc::new((in_dir, out_dir));
    // Spawn file creation tasks
    while let Some(dir_entry) = read_dir.next_entry().await? {
        let dirs = Arc::clone(&sd);
        tasks.push(tokio::spawn(async move {
            write_dir_entry(dirs, dir_entry).await
        }));
    }
    // Now await their completion
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
    info!("Completed in {time} sec",);
    Ok(())
}

#[inline]
#[expect(clippy::single_call_fn, reason = "Readability")]
async fn write_dir_entry<P: AsRef<Path> + Send + Sync>(
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
            info!("Packing '{}'", path.to_string_lossy());
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
        if path != dest {
            if log_enabled!(Level::Info) {
                info!(
                    "Copying '{}' to '{}'",
                    path.to_string_lossy(),
                    dest.to_string_lossy()
                );
            }
            copy_file(&dir_entry.path(), &dest).await?;
        }
        if dest
            .file_name()
            .is_some_and(|file_name| file_name.to_string_lossy().to_uppercase() == "SLPM_625.53")
            && let Some(exec_data_path) = find_json(in_dir)?
        {
            patch_exec(&dest, exec_data_path).await?;
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
async fn unpack(in_path: PathBuf, out_dir: PathBuf, copy_images: bool) -> Result<(), io::Error> {
    let now = Instant::now();
    fs::create_dir_all(&out_dir).await?;
    let mut tasks = Vec::with_capacity(16);
    let oda = Arc::new(out_dir);
    let mut read_dir = fs::read_dir(&in_path).await?;
    while let Some(dir_entry) = read_dir.next_entry().await? {
        let od = Arc::clone(&oda);
        tasks.push(tokio::spawn(async move {
            read_dir_entry(od, copy_images, dir_entry).await
        }));
    }
    while !tasks.is_empty() {
        for i in 0..tasks.len() {
            if tasks.get(i).is_some_and(JoinHandle::is_finished) {
                let task = tasks.remove(i);
                task.await??;
            }
        }
        sleep(Duration::from_millis(100)).await;
    }
    #[expect(clippy::float_arithmetic, reason = "it's only for display")]
    let time = f64::from(u32::try_from(now.elapsed().as_millis()).unwrap()) / 1_000f64;
    info!("Completed in {time} sec",);
    Ok(())
}

async fn read_dir_entry<P: AsRef<Path> + Send + Sync>(
    out_dir: Arc<P>,
    copy_images: bool,
    dir_entry: fs::DirEntry,
) -> Result<(), io::Error> {
    let path = dir_entry.path();
    let dest = (*out_dir).as_ref().join(path.file_name().unwrap());
    if path.to_string_lossy().ends_with("SLPM_625.53") {
        generate_exec_data(&*out_dir, &path).await?;
    }
    if path.is_dir() {
        copy_dir_all(&path, &dest).await?;
        return Ok(());
    } else if path
        .extension()
        .is_some_and(|stem| stem.to_string_lossy().ends_with("DAT"))
    {
        info!(
            "Unpacking '{}' to {}",
            path.to_string_lossy(),
            dest.to_string_lossy()
        );
        let dat_file = fs::File::open(path).await?;
        let dat_file_size = dat_file.metadata().await?.len().try_into().unwrap();
        let mut dat_reader = BufReader::new(dat_file);
        unpack_dat(
            &mut dat_reader,
            dir_entry.file_name().as_os_str(),
            dat_file_size,
            out_dir,
            copy_images,
        )
        .await?;
        return Ok(());
    }
    info!(
        "Copying '{}' to '{}'",
        path.to_string_lossy(),
        dest.to_string_lossy()
    );
    copy_file(&dir_entry.path(), &dest).await?;

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

#![allow(clippy::arbitrary_source_item_ordering, reason = "not needed")]
pub mod end_credits;
pub mod enemies;
pub mod items;
use crate::{
    events::load_exec_patch,
    helpers::{save_binary_file, unset_readonly},
    slpm_patcher::{end_credits::EndCreditItem, enemies::EnemyInfo, items::ItemInfo},
};
use log::{Level, info, log_enabled};
use serde::{Deserialize, Serialize};
use std::{
    io,
    path::{Path, PathBuf},
};
use strum::EnumIter;
use tokio::{
    fs::{self, OpenOptions},
    io::{AsyncWriteExt, BufReader, BufWriter},
};

// Via `readelf d:\SLPM_625.53 -l`
// Program segment 1 and 2 both end up as 0xFF000
static VMA_OFFSET: usize = 0x10_0000;
static FILE_OFFSET: usize = 0x1000;
pub static POINTER_OFFSET: usize = VMA_OFFSET - FILE_OFFSET;

// static MAPNAMES_JUMPLIST_START: usize = 0x14_F798;
// static MAPNAMES_POINTER_COUNT: usize = 106;

#[derive(Serialize, Deserialize)]
pub struct ExecData {
    pub items: Box<[ItemInfo]>,
    pub enemies: Box<[EnemyInfo]>,
    // pub strings: Vec<String>,
    pub end_credits: Box<[EndCreditItem]>,
}

#[repr(u8)]
#[derive(EnumIter, Serialize, Deserialize, Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Debug)]
enum Elemental {
    Fire = 0x01,
    Ice = 0x02,
    Air = 0x04,
    Lightning = 0x08,
}

#[inline]
pub async fn generate_exec_data<P: AsRef<Path> + Send + Sync>(
    path: &PathBuf,
    out_dir: &P,
) -> Result<(), io::Error> {
    let elf_file = fs::File::open(path).await?;
    let mut elf_reader = BufReader::new(elf_file);
    let exec_data = ExecData {
        items: items::parse(&mut elf_reader).await?.into_boxed_slice(),
        enemies: enemies::parse(&mut elf_reader).await?.into_boxed_slice(),
        // strings: parse_map_strings(&mut elf_reader).await?,
        end_credits: end_credits::parse(&mut elf_reader)
            .await?
            .into_boxed_slice(),
    };
    let save_path = PathBuf::with_capacity(128)
        .join(out_dir)
        .join("exec_data.json");
    save_binary_file(
        &save_path,
        serde_json::to_string_pretty(&exec_data).unwrap().as_bytes(),
    )
    .await?;
    Ok(())
}

#[inline]
pub async fn patch_exec(dest: &PathBuf, exec_data_path: PathBuf) -> Result<(), io::Error> {
    if log_enabled!(Level::Info) {
        info!("Patching '{}'", dest.to_string_lossy());
    }
    let ExecData {
        items,
        enemies,
        end_credits,
    } = load_exec_patch(exec_data_path)?;
    unset_readonly(dest).await?;
    let elf_binary = OpenOptions::new().write(true).open(dest).await?;
    let mut bw = BufWriter::new(elf_binary);
    items::patch(&mut bw, items).await?;
    enemies::patch(&mut bw, enemies).await?;
    end_credits::patch(&mut bw, end_credits).await?;
    bw.flush().await?;
    Ok(())
}

// use crate::helpers::{deserialize_u32_hex, serialize_u32_hex};
// #[derive(Serialize, Deserialize, PartialEq, PartialOrd, Eq, Ord)]
// struct Hexu32(
//     #[serde(
//         serialize_with = "serialize_u32_hex",
//         deserialize_with = "deserialize_u32_hex"
//     )]
//     u32,
// );

// pub async fn parse_map_strings<R: AsyncBufRead + AsyncSeek + Unpin>(reader: &mut R) -> Result<Vec<String>, io::Error> {
//     reader
//         .seek(SeekFrom::Start(MAPNAMES_JUMPLIST_START as u64))
//         .await?;
//     let mut pointer_bytes = [0u8; 4];
//     let mut pointer_vec = Vec::with_capacity(MAPNAMES_POINTER_COUNT);
//     let mut mapnames = Vec::with_capacity(MAPNAMES_POINTER_COUNT);
//     for _ in 0..MAPNAMES_POINTER_COUNT {
//         reader.read_exact(&mut pointer_bytes).await?;
//         pointer_vec.push(u32::from_le_bytes(pointer_bytes));
//     }
//     for pointer in pointer_vec {
//         reader
//             .seek(SeekFrom::Start(u64::from(pointer) - POINTER_OFFSET as u64))
//             .await?;
//         let mut string_bytes = Vec::with_capacity(20);
//         reader.read_until(0, &mut string_bytes).await?;
//         let mut string_bytes_iter = string_bytes.into_iter().peekable();
//         let mut engrish_str = Vec::with_capacity(20);
//         while let Some(byte) = string_bytes_iter.next()
//             && byte != 0
//         {
//             parse_next_event_char(&mut string_bytes_iter, &mut engrish_str, byte);
//         }
//         let mut mapname = engrish_str.concat();
//         mapname.shrink_to_fit();
//         mapnames.push(mapname);
//     }
//     mapnames.shrink_to_fit();
//     Ok(mapnames)
// }

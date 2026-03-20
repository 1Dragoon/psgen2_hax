#![allow(clippy::arbitrary_source_item_ordering, reason = "not needed")]
pub mod end_credits;
pub mod enemies;
pub mod items;
use crate::{
    events::{codec::decode_psg2_string, load_exec_patch},
    helpers::{is_default, save_binary_file, unset_readonly},
    slpm_patcher::{end_credits::EndCreditItem, enemies::EnemyInfo, items::ItemInfo},
};
use alloc::collections::BTreeMap;
// use indexmap::IndexMap;
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

static MAPNAMES_JUMPLIST_START: usize = 0x14_F798;
static MAPNAMES_JUMPLIST_FIELDS: usize = 107;
// static MAPNAMES_STRING_REGION_START: usize = 0x18_8770;
// static MAPNAMES_STRING_REGION_END: usize = 0x18_90AF;

static DUNNO_JUMPLIST_START: usize = 0x15_5778;
static DUNNO_JUMPLIST_FIELDS: usize = 97;

static MENU_TEXT_JUMPLIST_START: usize = 0x16_51D0;
static MENU_TEXT_JUMPLIST_FIELDS: usize = 149;

static ITEM_DESCRIPTION_JUMPLIST_START: usize = 0x16_5428;
static ITEM_DESCRIPTION_JUMPLIST_FIELDS: usize = 195;

// static STRING_REGION_A_START: usize = 0x1A_9AF0; // menu text
// static STRING_REGION_A_END: usize = 0x1A_D802; //

// static ENEMY_NAMES_STRING_REGION_START: usize = 0x1A_89E0;
// static ENEMY_NAMES_STRING_REGION_END: usize = 0x1A_929F;

static TECHNIQUE_STRUCT_START: usize = 0x1A_28A0;
static TECHNIQUE_STRUCT_COUNT: usize = 83;
static TECHNIQUE_STRUCT_FIELDS: usize = 14;

static MUSIC_STRUCT_START: usize = 0x18_9450;
static MUSIC_STRUCT_COUNT: usize = 19;
static MUSIC_STRUCT_FIELDS: usize = 2;

// static DUNNO_STRUCT_START: usize = 0x1A_28A0; // end 1A3AC8
// static DUNNO_STRUCT_COUNT: usize = 186;
// static DUNNO_STRUCT_FIELDS: usize = 14;

#[derive(Serialize, Deserialize)]
pub struct Song {
    #[serde(
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex"
    )]
    number: u32,
    name: DialogString,
    #[serde(
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex"
    )]
    name_pointer: u32,
    #[serde(
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex"
    )]
    field_1: u32,
}

#[derive(Serialize, Deserialize)]
pub struct Technique {
    #[serde(
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex"
    )]
    tech_number: u32,
    name: DialogString,
    #[serde(
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex"
    )]
    name_pointer: u32,
    #[serde(
        default,
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex",
        skip_serializing_if = "is_default"
    )]
    field_1: u32,
    #[serde(
        default,
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex",
        skip_serializing_if = "is_default"
    )]
    field_2: u32,
    #[serde(
        default,
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex",
        skip_serializing_if = "is_default"
    )]
    field_3: u32,
    #[serde(
        default,
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex",
        skip_serializing_if = "is_default"
    )]
    field_4: u32,
    #[serde(
        default,
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex",
        skip_serializing_if = "is_default"
    )]
    field_5: u32,
    #[serde(
        default,
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex",
        skip_serializing_if = "is_default"
    )]
    field_6: u32,
    #[serde(
        default,
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex",
        skip_serializing_if = "is_default"
    )]
    field_7: u32,
    #[serde(
        default,
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex",
        skip_serializing_if = "is_default"
    )]
    field_8: u32,
    #[serde(
        default,
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex",
        skip_serializing_if = "is_default"
    )]
    field_9: u32,
    #[serde(
        default,
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex",
        skip_serializing_if = "is_default"
    )]
    field_10: u32,
    #[serde(
        default,
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex",
        skip_serializing_if = "is_default"
    )]
    field_11: u32,
    #[serde(
        default,
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex",
        skip_serializing_if = "is_default"
    )]
    field_12: u32,
    #[serde(
        default,
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex",
        skip_serializing_if = "is_default"
    )]
    field_13: u32,
}

// pub async fn parse_structs<R: AsyncBufRead + AsyncSeek + Unpin>(
//     reader: &mut R,
//     location: usize,
//     field_count: usize,
//     struct_count: usize,
//     text_field: usize,
// ) -> Result<IndexMap<Hexu32, (DialogString, Vec<Hexu32>)>, io::Error> {
//     reader.seek(SeekFrom::Start(location as u64)).await?;
//     let mut field_bytes = [0u8; 4];
//     let mut structs = Vec::with_capacity(struct_count);
//     for _ in 0..struct_count {
//         let mut field_vec = Vec::with_capacity(field_count);
//         for _ in 0..field_count {
//             reader.read_exact(&mut field_bytes).await?;
//             field_vec.push(Hexu32(u32::from_be_bytes(field_bytes)));
//         }
//         structs.push(field_vec);
//     }
//     let mut index_map = IndexMap::with_capacity(struct_count);
//     for (i, fields) in structs.into_iter().enumerate() {
//         reader
//             .seek(SeekFrom::Start(
//                 u64::from(u32::from_le_bytes(fields[text_field].0.to_be_bytes()))
//                     - POINTER_OFFSET as u64,
//             ))
//             .await?;
//         let mut string_bytes = Vec::with_capacity(20);
//         while let Ok(byte) = reader.read_u8().await
//             && byte != 0
//         {
//             string_bytes.push(byte);
//         }
//         index_map.insert(
//             Hexu32(u32::try_from(i).unwrap()),
//             (decode_psg2_string(string_bytes), fields),
//         );
//     }

//     Ok(index_map)
// }

pub async fn parse_songs<R: AsyncBufRead + AsyncSeek + Unpin>(
    reader: &mut R,
) -> Result<Box<[Song]>, io::Error> {
    reader
        .seek(SeekFrom::Start(MUSIC_STRUCT_START as u64))
        .await?;
    let mut field_bytes = [0u8; 4];
    let mut songs = Vec::with_capacity(MUSIC_STRUCT_COUNT);
    for i in 0..MUSIC_STRUCT_COUNT {
        let mut fields = Vec::with_capacity(MUSIC_STRUCT_FIELDS);
        for _ in 0..MUSIC_STRUCT_FIELDS {
            reader.read_exact(&mut field_bytes).await?;
            fields.push(field_bytes);
        }
        let song = Song {
            number: u32::try_from(i).unwrap(),
            name: DialogString::default(),
            name_pointer: u32::from_le_bytes(fields.pop().unwrap()),
            field_1: u32::from_le_bytes(fields.pop().unwrap()),
        };
        songs.push(song);
    }

    for song in &mut songs {
        reader
            .seek(SeekFrom::Start(
                u64::from(song.name_pointer) - POINTER_OFFSET as u64,
            ))
            .await?;
        let mut string_bytes = Vec::with_capacity(20);
        while let Ok(byte) = reader.read_u8().await
            && byte != 0
        {
            string_bytes.push(byte);
        }
        song.name = decode_psg2_string(string_bytes);
    }

    Ok(songs.into_boxed_slice())
}

pub async fn parse_techniques<R: AsyncBufRead + AsyncSeek + Unpin>(
    reader: &mut R,
) -> Result<Box<[Technique]>, io::Error> {
    reader
        .seek(SeekFrom::Start(TECHNIQUE_STRUCT_START as u64))
        .await?;
    let mut field_bytes = [0u8; 4];
    let mut techniques = Vec::with_capacity(TECHNIQUE_STRUCT_COUNT);
    for i in 0..TECHNIQUE_STRUCT_COUNT {
        let mut fields = Vec::with_capacity(TECHNIQUE_STRUCT_FIELDS);
        for _ in 0..TECHNIQUE_STRUCT_FIELDS {
            reader.read_exact(&mut field_bytes).await?;
            fields.push(field_bytes);
        }
        fields.reverse();
        let technique = Technique {
            tech_number: u32::try_from(i).unwrap(),
            name: DialogString::default(),
            name_pointer: u32::from_le_bytes(fields.pop().unwrap()),
            field_1: u32::from_le_bytes(fields.pop().unwrap()),
            field_2: u32::from_le_bytes(fields.pop().unwrap()),
            field_3: u32::from_le_bytes(fields.pop().unwrap()),
            field_4: u32::from_le_bytes(fields.pop().unwrap()),
            field_5: u32::from_le_bytes(fields.pop().unwrap()),
            field_6: u32::from_le_bytes(fields.pop().unwrap()),
            field_7: u32::from_le_bytes(fields.pop().unwrap()),
            field_8: u32::from_le_bytes(fields.pop().unwrap()),
            field_9: u32::from_le_bytes(fields.pop().unwrap()),
            field_10: u32::from_le_bytes(fields.pop().unwrap()),
            field_11: u32::from_le_bytes(fields.pop().unwrap()),
            field_12: u32::from_le_bytes(fields.pop().unwrap()),
            field_13: u32::from_le_bytes(fields.pop().unwrap()),
        };

        techniques.push(technique);
    }

    for technique in &mut techniques {
        reader
            .seek(SeekFrom::Start(
                u64::from(technique.name_pointer) - POINTER_OFFSET as u64,
            ))
            .await?;
        let mut string_bytes = Vec::with_capacity(20);
        while let Ok(byte) = reader.read_u8().await
            && byte != 0
        {
            string_bytes.push(byte);
        }
        technique.name = decode_psg2_string(string_bytes);
    }

    Ok(techniques.into_boxed_slice())
}

#[derive(Serialize, Deserialize)]
pub struct ExecData {
    pub mapnames: BTreeMap<Hexu32, DialogString>,
    pub menu_text: BTreeMap<Hexu32, DialogString>,
    pub item_descriptions: BTreeMap<Hexu32, DialogString>,
    pub dunno_jumplist: BTreeMap<Hexu32, DialogString>,
    // pub dunno_struct: IndexMap<Hexu32, (DialogString, Vec<Hexu32>)>,
    pub techniques: Box<[Technique]>,
    pub songs: Box<[Song]>,
    pub items: Box<[ItemInfo]>,
    pub enemies: Box<[EnemyInfo]>,
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
        mapnames: parse_jumplist_strings(
            &mut elf_reader,
            MAPNAMES_JUMPLIST_START,
            MAPNAMES_JUMPLIST_FIELDS,
        )
        .await?,
        menu_text: parse_jumplist_strings(
            &mut elf_reader,
            MENU_TEXT_JUMPLIST_START,
            MENU_TEXT_JUMPLIST_FIELDS,
        )
        .await?,
        item_descriptions: parse_jumplist_strings(
            &mut elf_reader,
            ITEM_DESCRIPTION_JUMPLIST_START,
            ITEM_DESCRIPTION_JUMPLIST_FIELDS,
        )
        .await?,
        // dunno_struct: parse_structs(&mut elf_reader, DUNNO_STRUCT_START, DUNNO_STRUCT_FIELDS, DUNNO_STRUCT_COUNT, 0).await?,
        dunno_jumplist: parse_jumplist_strings(
            &mut elf_reader,
            DUNNO_JUMPLIST_START,
            DUNNO_JUMPLIST_FIELDS,
        )
        .await?,
        techniques: parse_techniques(&mut elf_reader).await?,
        songs: parse_songs(&mut elf_reader).await?,
        items: items::parse(&mut elf_reader).await?.into_boxed_slice(),
        enemies: enemies::parse(&mut elf_reader).await?.into_boxed_slice(),
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
        mapnames: _a,
        menu_text: _b,
        item_descriptions: _c,
        dunno_jumplist: _d,
        // dunno_struct: _e,
        techniques: _f,
        songs: _g,
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

use crate::helpers::{deserialize_u32_hex, serialize_u32_hex};
#[derive(Serialize, Deserialize, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub struct Hexu32(
    #[serde(
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex"
    )]
    u32,
);

use crate::events::DialogString;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncSeek, AsyncSeekExt, SeekFrom};

pub async fn parse_jumplist_strings<R: AsyncBufRead + AsyncSeek + Unpin>(
    reader: &mut R,
    location: usize,
    count: usize,
) -> Result<BTreeMap<Hexu32, DialogString>, io::Error> {
    reader.seek(SeekFrom::Start(location as u64)).await?;
    let mut pointer_bytes = [0u8; 4];
    let mut pointer_vec = Vec::with_capacity(count);
    let mut mapnames = BTreeMap::new();
    for _ in 0..count {
        reader.read_exact(&mut pointer_bytes).await?;
        pointer_vec.push(u32::from_le_bytes(pointer_bytes));
    }
    assert_eq!(
        pointer_vec.len(),
        count,
        "Number of string items must be EXACT!"
    );
    for pointer in pointer_vec {
        reader
            .seek(SeekFrom::Start(u64::from(pointer) - POINTER_OFFSET as u64))
            .await?;
        let mut string_bytes = Vec::with_capacity(20);
        reader.read_until(0, &mut string_bytes).await?;
        let mut string_bytes_iter = string_bytes.into_iter();
        let mut engrish_bytes = Vec::with_capacity(128);
        while let Some(byte) = string_bytes_iter.next()
            && byte != 0
        {
            engrish_bytes.push(byte);
        }
        let engrish_str = decode_psg2_string(engrish_bytes);
        mapnames.insert(
            Hexu32(pointer - u32::try_from(POINTER_OFFSET).unwrap()),
            engrish_str,
        );
    }
    Ok(mapnames)
}

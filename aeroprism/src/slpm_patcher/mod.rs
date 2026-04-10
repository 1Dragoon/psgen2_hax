#![allow(clippy::arbitrary_source_item_ordering, reason = "not needed")]
pub mod end_credits;
pub mod enemies;
pub mod items;
pub mod techniques;
use crate::{
    EXEC_STRUCTURES_FILENAME,
    events::{
        DialogItem, DialogString, codec::decode_psg2_string, deserialize_dialog_items,
        load_exec_struct_patch, serialize_dialog_items,
    },
    helpers::{
        deserialize_u32_hex, encode_hex, is_default, save_binary_file, serialize_u32_hex,
        unset_readonly,
    },
    slpm_patcher::{
        end_credits::EndCreditItem, enemies::EnemyInfo, items::ItemInfo, techniques::Technique,
    },
};
use alloc::collections::BTreeMap;
use indexmap::IndexSet;
use itertools::Itertools;
use log::{Level, debug, error, info, log_enabled, warn};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    io,
    path::{Path, PathBuf},
};
use strum::{EnumIter, IntoEnumIterator};
use tokio::{
    fs::{self, OpenOptions},
    io::{
        AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncSeek, AsyncSeekExt, AsyncWriteExt,
        BufReader, BufWriter, SeekFrom,
    },
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

static MISC_STRINGS_JUMPLIST_START: usize = 0x15_5778;
static MISC_STRINGS_JUMPLIST_FIELDS: usize = 97;

static MENU_TEXT_JUMPLIST_START: usize = 0x16_51D0;
static MENU_TEXT_JUMPLIST_FIELDS: usize = 149;

static ITEM_DESCRIPTION_JUMPLIST_START: usize = 0x16_5428;
static ITEM_DESCRIPTION_JUMPLIST_FIELDS: usize = 195;

static SAVEXIT_JUMPLIST_START: usize = 0x15_65B0;
static SAVEXIT_JUMPLIST_FIELDS: usize = 3;

// static INDICATORS_JUMPLIST_START: usize = 0x18_B044;
// static INDICATORS_JUMPLIST_FIELDS: usize = 6;

// static DUNNO_JUMPLIST_START: usize = 0x16_1868;
// static DUNNO_JUMPLIST_FIELDS: usize = 1;

static QUESTION_JUMPLIST_START: usize = 0x16_1868;
static QUESTION_JUMPLIST_FIELDS: usize = 1;

static MEMCARD_STRUCT_START: usize = 0x18_E0A0;
static MEMCARD_STRUCT_COUNT: usize = 9;
static MEMCARD_STRUCT_FIELDS: usize = 2;

static MUSIC_STRUCT_START: usize = 0x18_9450;
static MUSIC_STRUCT_COUNT: usize = 19;
static MUSIC_STRUCT_FIELDS: usize = 2;

static TECHNIQUE_STRUCT_START: usize = 0x1A_28A0;
static TECHNIQUE_STRUCT_COUNT: usize = 83;
static TECHNIQUE_STRUCT_FIELDS: usize = 14;

static ITEM_STRUCTS_START: usize = 0x18_B1B0;
static ITEM_STRUCT_COUNT: usize = 186;
static ITEM_STRUCT_FIELDS: usize = 8;

// static DUNNO_STRUCT_START: usize = 0x1A4198; // end 1A3AC8
// static DUNNO_STRUCT_COUNT: usize = 9;
// static DUNNO_STRUCT_FIELDS: usize = 25;

#[derive(
    Serialize, Deserialize, Ord, PartialOrd, Eq, PartialEq, Copy, Clone, Debug, Default, Hash,
)]
pub enum MemRegion {
    #[serde(rename = "ja")]
    JumplessA,
    #[default]
    #[serde(rename = "sa")]
    StructuredA,
    #[serde(rename = "jb")]
    JumplessB,
    #[serde(rename = "jc")]
    JumplessC,
    #[serde(rename = "sb")]
    StructuredB,
    #[serde(rename = "jd")]
    JumplessD,
    #[serde(rename = "je")]
    JumplessE,
    #[serde(rename = "jf")]
    JumplessF,
    #[serde(rename = "jg")]
    JumplessG,
    #[serde(rename = "jh")]
    JumplessH,
    #[serde(rename = "sc")]
    StructuredC,
    #[serde(rename = "ji")]
    JumplessI,
    #[serde(rename = "sd")]
    StructuredD,
    #[serde(rename = "jj")]
    JumplessJ,
    #[serde(rename = "jk")]
    JumplessK,
    #[serde(rename = "se")]
    StructuredE,
    #[serde(rename = "sf")]
    StructuredF,
    #[serde(rename = "sg")]
    StructuredG,
    #[serde(rename = "sh")]
    StructuredH,
    #[serde(rename = "si")]
    StructuredI,
    #[serde(rename = "sj")]
    StructuredJ,
    #[serde(rename = "sk")]
    StructuredK,
    #[serde(rename = "sl")]
    StructuredL,
    #[serde(rename = "sm")]
    StructuredM,
}

impl MemRegion {
    const fn offset_size(self) -> (usize, usize) {
        match self {
            Self::JumplessA => (0x18_7880, 0x50),
            Self::StructuredA => (0x18_8770, 0x940),
            Self::JumplessB => (0x18_9210, 0x90),
            Self::JumplessC => (0x18_92C8, 0x40),
            Self::StructuredB => (0x18_94E8, 0x1a8),
            Self::JumplessD => (0x18_96E8, 0x10),
            Self::JumplessE => (0x18_A330, 0x420),
            Self::JumplessF => (0x18_A760, 0x48),
            Self::JumplessG => (0x18_B0A0, 0x30),
            Self::JumplessH => (0x18_B160, 0x18),
            Self::StructuredC => (0x18_C8F0, 0x10e8),
            Self::JumplessI => (0x18_DA10, 0x378),
            Self::StructuredD => (0x18_DF18, 0x188),
            Self::JumplessJ => (0x1A_1C80, 0x48),
            Self::JumplessK => (0x1A_1E78, 0x470),
            Self::StructuredE => (0x1A_3AC8, 0x450),
            Self::StructuredF => (0x1A_89E0, 0x8c0),
            Self::StructuredG => (0x1A_9AF0, 0x3d18),
            Self::StructuredH => (0x1B_1840, 0x100),
            Self::StructuredI => (0x1D_B218, 0x18),
            Self::StructuredJ => (0x1D_B238, 0x98),
            Self::StructuredK => (0x1D_B2D8, 0x1B0),
            Self::StructuredL => (0x1D_B598, 0xb0),
            Self::StructuredM => (0x1D_B680, 0x128),
        }
    }
}

// Note: Some of my string regions are smaller than the goldenboy release because I want to avoid writing into the trailing 64-bit null fields.
impl TryFrom<u32> for MemRegion {
    type Error = String;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            0x18_7880..0x18_78D0 => Ok(Self::JumplessA),
            0x18_8770..0x18_90B0 => Ok(Self::StructuredA),
            0x18_9210..0x18_92A0 => Ok(Self::JumplessB),
            0x18_92C8..0x18_9308 => Ok(Self::JumplessC), // Goldenboy goes to 0x189310
            0x18_94E8..0x18_9690 => Ok(Self::StructuredB),
            0x18_96E8..0x18_96F8 => Ok(Self::JumplessD), // Goldenboy goes to 0x189700
            0x18_A330..0x18_A750 => Ok(Self::JumplessE),
            0x18_A760..0x18_A7A8 => Ok(Self::JumplessF), // Goldenboy goes to 0x18A7B0
            0x18_B0A0..0x18_B0D0 => Ok(Self::JumplessG),
            0x18_B160..0x18_B178 => Ok(Self::JumplessH),
            0x18_C8F0..0x18_D9D8 => Ok(Self::StructuredC), // Goldenboy goes to 0x18D9E0
            0x18_DA10..0x18_DD88 => Ok(Self::JumplessI),   // Goldenboy goes to 0x18DD90
            0x18_DF18..0x18_E0A0 => Ok(Self::StructuredD),
            0x1A_1C80..0x1A_1CC8 => Ok(Self::JumplessJ), // Goldenboy goes to 0x1A1CD0
            0x1A_1E78..0x1A_22E8 => Ok(Self::JumplessK),
            0x1A_3AC8..0x1A_3F18 => Ok(Self::StructuredE),
            0x1A_89E0..0x1A_9298 => Ok(Self::StructuredF),
            0x1A_9AF0..0x1A_D808 => Ok(Self::StructuredG),
            0x1B_1840..0x1B_1940 => Ok(Self::StructuredH),
            0x1D_B218..0x1D_B230 => Ok(Self::StructuredI),
            0x1D_B238..0x1D_B2D0 => Ok(Self::StructuredJ),
            0x1D_B2D8..0x1D_B488 => Ok(Self::StructuredK),
            0x1D_B598..0x1D_B648 => Ok(Self::StructuredL),
            0x1D_B680..0x1D_B7A8 => Ok(Self::StructuredM),
            _ => Err(format!(
                "No defined memory region for address 0x{value:06x}"
            )),
        }
    }
}

#[repr(u8)]
#[derive(EnumIter, Serialize, Deserialize, Eq, PartialEq, Copy, Clone)]
pub enum Character {
    #[serde(alias = "eusis")]
    Eusis = 0x01,
    #[serde(alias = "nei")]
    Nei = 0x02,
    #[serde(alias = "rudger")]
    Rudger = 0x04,
    #[serde(alias = "anne")]
    Anne = 0x08,
    #[serde(alias = "huey")]
    Huey = 0x10,
    #[serde(alias = "amia")]
    Amia = 0x20,
    #[serde(alias = "keinz")]
    Keinz = 0x40,
    #[serde(alias = "silka")]
    Silka = 0x80,
}

impl Character {
    fn from_byte(byte: u8) -> Box<[Self]> {
        let mut variants = Vec::with_capacity(8);
        for variant in Self::iter() {
            if byte & variant as u8 == variant as u8 {
                variants.push(variant);
            }
        }
        variants.into_boxed_slice()
    }

    fn to_byte(value: &[Self]) -> u8 {
        let mut byte = 0;
        for variant in value.iter().copied() {
            byte |= variant as u8;
        }
        byte
    }
}

#[derive(Serialize, Deserialize, Default, Copy, Clone)]
pub struct RelativePointerInfo {
    #[serde(default, skip_serializing_if = "is_default")]
    pub mem_region: MemRegion,
    #[serde(default, skip_serializing_if = "is_default")]
    pub position: Hexu32,
    #[serde(default, skip_serializing_if = "is_default")]
    pub alias: Option<(MemRegion, Hexu32)>,
}

impl RelativePointerInfo {
    const fn new(
        mem_region: MemRegion,
        position: Hexu32,
        alias: Option<(MemRegion, Hexu32)>,
    ) -> Self {
        Self {
            mem_region,
            position,
            alias,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct Song {
    #[serde(
        deserialize_with = "deserialize_dialog_items",
        serialize_with = "serialize_dialog_items"
    )]
    name: Vec<DialogItem>,
    #[serde(
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex"
    )]
    name_pointer: u32,
    #[serde(
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex"
    )]
    name_vma_pointer: u32,
    #[serde(skip)]
    text: DialogString,
    #[serde(flatten)]
    relative_name_pointer: RelativePointerInfo,
    unknown_1: i32,
}

impl StringFill for Song {
    fn get_relative_pointer(&self) -> RelativePointerInfo {
        self.relative_name_pointer
    }

    fn get_text(&'_ self) -> &'_ DialogString {
        &self.text
    }

    fn pad_text(&mut self, size: u8) {
        self.text.set_padding(size);
    }

    fn set_vma_pointer(&mut self, ptr_le: u32) {
        if self.name_vma_pointer != ptr_le {
            warn!(
                "Got {}, expected {}",
                encode_hex(&ptr_le.to_le_bytes()),
                encode_hex(&self.name_vma_pointer.to_le_bytes())
            );
        }
        self.name_vma_pointer = ptr_le;
    }

    fn convert_text(&mut self) {
        self.text = DialogString {
            text: self.name.clone(),
            padding: 0,
        };
    }
}

#[derive(Serialize, Deserialize)]
pub struct MemcardOpt {
    #[serde(
        deserialize_with = "deserialize_dialog_items",
        serialize_with = "serialize_dialog_items"
    )]
    string: Vec<DialogItem>,
    #[serde(
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex"
    )]
    name_pointer: u32,
    #[serde(
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex"
    )]
    name_vma_pointer: u32,
    #[serde(skip)]
    text: DialogString,
    #[serde(flatten)]
    relative_name_pointer: RelativePointerInfo,
    unknown_1: i32,
}

impl StringFill for MemcardOpt {
    fn get_relative_pointer(&self) -> RelativePointerInfo {
        self.relative_name_pointer
    }

    fn get_text(&'_ self) -> &'_ DialogString {
        &self.text
    }

    fn pad_text(&mut self, size: u8) {
        self.text.set_padding(size);
    }

    fn set_vma_pointer(&mut self, ptr_le: u32) {
        if self.name_vma_pointer != ptr_le {
            warn!(
                "Got {}, expected {}",
                encode_hex(&ptr_le.to_le_bytes()),
                encode_hex(&self.name_vma_pointer.to_le_bytes())
            );
        }
        self.name_vma_pointer = ptr_le;
    }

    fn convert_text(&mut self) {
        self.text = DialogString {
            text: self.string.clone(),
            padding: 0,
        };
    }
}

const JUMPLESS_REGIONS: [MemRegion; 11] = [
    MemRegion::JumplessA,
    MemRegion::JumplessB,
    MemRegion::JumplessC,
    MemRegion::JumplessD,
    MemRegion::JumplessE,
    MemRegion::JumplessF,
    MemRegion::JumplessG,
    MemRegion::JumplessH,
    MemRegion::JumplessI,
    MemRegion::JumplessJ,
    MemRegion::JumplessK,
];

pub async fn parse_jumpless<R: AsyncBufRead + AsyncSeek + Unpin>(
    reader: &mut R,
) -> Result<BTreeMap<MemRegion, Vec<DialogString>>, io::Error> {
    let mut regions = BTreeMap::new();
    for region in JUMPLESS_REGIONS {
        debug!("Reading string region {region:?}...");
        let (offset, size) = region.offset_size();
        reader.seek(SeekFrom::Start(offset as u64)).await?;
        let mut region_bytes = vec![0u8; size];
        reader.read_exact(&mut region_bytes).await?;
        let mut strings = Vec::with_capacity(128);
        while !region_bytes.is_empty() {
            debug!("Region bytes {} left", region_bytes.len());
            let (terminator_pos, _) = region_bytes.iter().find_position(|b| **b == 0).unwrap();
            let bytes = region_bytes.drain(0..terminator_pos).collect_vec();
            strings.push(decode_psg2_string(bytes));
            if let Some((non_terminator_pos, _)) = region_bytes.iter().find_position(|b| **b != 0) {
                // Remove any leftover padding
                let f = region_bytes.drain(0..non_terminator_pos).collect_vec();
                debug!("Removed {} bytes of padding", f.len());
            } else {
                break;
            }
        }
        regions.insert(region, strings);
    }
    Ok(regions)
}

// pub async fn parse_structs<R: AsyncBufRead + AsyncSeek + Unpin>(
//     reader: &mut R,
//     location: usize,
//     field_count: usize,
//     struct_count: usize,
//     text_field: usize,
// ) -> Result<indexmap::IndexMap<Hexu32, (DialogString, Vec<Hexu32>)>, io::Error> {
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
//     let mut index_map = indexmap::IndexMap::with_capacity(struct_count);
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
    pointers: &mut BTreeMap<u32, (MemRegion, Hexu32)>,
) -> Result<BTreeMap<Hexu32, Song>, io::Error> {
    reader
        .seek(SeekFrom::Start(MUSIC_STRUCT_START as u64))
        .await?;
    let mut field_bytes = [0u8; 4];
    let mut songs = BTreeMap::new();
    let mut relative_pointer_index = IndexSet::with_capacity(MUSIC_STRUCT_COUNT);
    for song_no in 0..MUSIC_STRUCT_COUNT {
        let mut fields = Vec::with_capacity(MUSIC_STRUCT_FIELDS);
        for _ in 0..MUSIC_STRUCT_FIELDS {
            reader.read_exact(&mut field_bytes).await?;
            fields.push(field_bytes);
        }
        let pointer_bytes = fields.pop().unwrap();
        let name_pointer =
            u32::from_le_bytes(pointer_bytes) - u32::try_from(POINTER_OFFSET).unwrap();
        relative_pointer_index.insert(name_pointer);
        let song = Song {
            name: Vec::new(),
            name_pointer,
            name_vma_pointer: u32::from_be_bytes(pointer_bytes),
            text: DialogString::default(),
            relative_name_pointer: RelativePointerInfo::default(),
            unknown_1: i32::from_le_bytes(fields.pop().unwrap()),
        };
        songs.insert(Hexu32(u32::try_from(song_no).unwrap()), song);
    }
    relative_pointer_index.sort_unstable();

    // let mut name_pointers = BTreeMap::new();

    for song in songs.values_mut() {
        let ptr = song.name_pointer;
        reader.seek(SeekFrom::Start(u64::from(ptr))).await?;
        let mut string_bytes = Vec::with_capacity(20);
        while let Ok(byte) = reader.read_u8().await
            && byte != 0
        {
            string_bytes.push(byte);
        }
        song.name = decode_psg2_string(string_bytes).text;
        let region = MemRegion::try_from(ptr).unwrap();
        let index = relative_pointer_index.get_index_of(&ptr).unwrap();
        let position = Hexu32(u32::try_from(index).unwrap());
        song.relative_name_pointer =
            RelativePointerInfo::new(region, position, pointers.get(&ptr).copied());
        pointers.insert(ptr, (region, position));
        // name_pointers.insert(
        //     Hexu32(song.name_pointer - 0xff000),
        //     (
        //         crate::helpers::encode_hex(&song.name_pointer.to_le_bytes()),
        //         song.name.to_string(),
        //     ),
        // );
    }
    // let bytes = serde_json::to_string_pretty(&name_pointers)
    //     .unwrap()
    //     .into_bytes();
    // save_binary_file(&PathBuf::from("jap_song_pointers.json"), &bytes).await?;

    Ok(songs)
}

#[inline]
pub async fn patch_songs(
    exec_writer: &mut BufWriter<fs::File>,
    songs: BTreeMap<Hexu32, Song>,
) -> Result<(), io::Error> {
    exec_writer
        .seek(SeekFrom::Start(MUSIC_STRUCT_START as u64))
        .await?;
    assert_eq!(
        MUSIC_STRUCT_COUNT,
        songs.len(),
        "Music count MUST be exact!"
    );
    for (_, song) in songs {
        // Field comes before name pointer here
        exec_writer.write_all(&song.unknown_1.to_le_bytes()).await?;
        exec_writer
            .write_all(&song.name_vma_pointer.to_be_bytes())
            .await?;
    }
    Ok(())
}

pub async fn parse_memcard_opts<R: AsyncBufRead + AsyncSeek + Unpin>(
    reader: &mut R,
    pointers: &mut BTreeMap<u32, (MemRegion, Hexu32)>,
) -> Result<BTreeMap<Hexu32, MemcardOpt>, io::Error> {
    reader
        .seek(SeekFrom::Start(MEMCARD_STRUCT_START as u64))
        .await?;
    let mut field_bytes = [0u8; 4];
    let mut memcard_opts = BTreeMap::new();
    let mut relative_pointer_index = IndexSet::with_capacity(MEMCARD_STRUCT_COUNT);
    for mc_opt_no in 0..MEMCARD_STRUCT_COUNT {
        let mut fields = Vec::with_capacity(MEMCARD_STRUCT_FIELDS);
        for _ in 0..MEMCARD_STRUCT_FIELDS {
            reader.read_exact(&mut field_bytes).await?;
            fields.push(field_bytes);
        }
        let pointer_bytes = fields.pop().unwrap();
        let name_pointer =
            u32::from_le_bytes(pointer_bytes) - u32::try_from(POINTER_OFFSET).unwrap();
        relative_pointer_index.insert(name_pointer);
        let memcard_opt = MemcardOpt {
            string: Vec::new(),
            text: DialogString::default(),
            name_pointer,
            name_vma_pointer: u32::from_be_bytes(pointer_bytes),
            relative_name_pointer: RelativePointerInfo::default(),
            unknown_1: i32::from_le_bytes(fields.pop().unwrap()),
        };
        memcard_opts.insert(Hexu32(u32::try_from(mc_opt_no).unwrap()), memcard_opt);
    }
    relative_pointer_index.sort_unstable();

    // let mut name_pointers = BTreeMap::new();

    for memcard_opt in memcard_opts.values_mut() {
        let ptr = memcard_opt.name_pointer;
        reader.seek(SeekFrom::Start(u64::from(ptr))).await?;
        let mut string_bytes = Vec::with_capacity(20);
        while let Ok(byte) = reader.read_u8().await
            && byte != 0
        {
            string_bytes.push(byte);
        }
        memcard_opt.string = decode_psg2_string(string_bytes).text;
        let region = MemRegion::try_from(ptr).unwrap();
        let index = relative_pointer_index.get_index_of(&ptr).unwrap();
        let position = Hexu32(u32::try_from(index).unwrap());
        memcard_opt.relative_name_pointer =
            RelativePointerInfo::new(region, position, pointers.get(&ptr).copied());
        pointers.insert(ptr, (region, position));
        // name_pointers.insert(
        //     Hexu32(memcard_opt.name_pointer - 0xff000),
        //     (
        //         crate::helpers::encode_hex(&memcard_opt.name_pointer.to_le_bytes()),
        //         memcard_opt.name.to_string(),
        //     ),
        // );
    }
    // let bytes = serde_json::to_string_pretty(&name_pointers)
    //     .unwrap()
    //     .into_bytes();
    // save_binary_file(&PathBuf::from("jap_mc_pointers.json"), &bytes).await?;

    Ok(memcard_opts)
}

#[inline]
pub async fn patch_memcard_opts(
    exec_writer: &mut BufWriter<fs::File>,
    memcard_opts: BTreeMap<Hexu32, MemcardOpt>,
) -> Result<(), io::Error> {
    exec_writer
        .seek(SeekFrom::Start(MEMCARD_STRUCT_START as u64))
        .await?;
    assert_eq!(
        MEMCARD_STRUCT_COUNT,
        memcard_opts.len(),
        "Music count MUST be exact!"
    );
    for (_, memcard_opt) in memcard_opts {
        // Field comes before name pointer here
        exec_writer
            .write_all(&memcard_opt.unknown_1.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&memcard_opt.name_vma_pointer.to_be_bytes())
            .await?;
    }
    Ok(())
}

#[derive(Serialize, Deserialize)]
pub struct ExecStructures {
    // pub dunno_struct: indexmap::IndexMap<Hexu32, (DialogString, Vec<Hexu32>)>,
    #[serde(rename = "technique")]
    pub techniques: BTreeMap<Hexu32, Technique>,
    #[serde(rename = "item")]
    pub items: BTreeMap<Hexu32, ItemInfo>,
    #[serde(rename = "enemy")]
    pub enemies: BTreeMap<Hexu32, EnemyInfo>,
    #[serde(rename = "end_credit")]
    pub end_credits: BTreeMap<usize, EndCreditItem>,
    #[serde(rename = "mapname")]
    pub mapnames: BTreeMap<Hexu32, JumplistItem>,
    pub menu_text: BTreeMap<Hexu32, JumplistItem>,
    #[serde(rename = "item_description")]
    pub item_descriptions: BTreeMap<Hexu32, JumplistItem>,
    #[serde(rename = "song")]
    pub songs: BTreeMap<Hexu32, Song>,
    #[serde(rename = "misc_string")]
    pub misc_strings: BTreeMap<Hexu32, JumplistItem>,
    #[serde(rename = "memcard_opt")]
    pub memcard_opts: BTreeMap<Hexu32, MemcardOpt>,
    pub savexit: BTreeMap<Hexu32, JumplistItem>,
    pub question: BTreeMap<Hexu32, JumplistItem>,
    pub other: BTreeMap<MemRegion, Vec<DialogString>>,
    // pub indicators: BTreeMap<Hexu32, JumplistItem>,
    // pub dunno: BTreeMap<Hexu32, JumplistItem>,
}

#[repr(u8)]
#[derive(
    EnumIter,
    Serialize,
    Deserialize,
    Default,
    Copy,
    Clone,
    PartialEq,
    PartialOrd,
    Eq,
    Ord,
    Hash,
    Debug,
)]
enum EnemyType {
    #[default]
    #[serde(alias = "demonic")]
    Demonic, // First and second bits turned off. Effectively, the below two bits count as a weakness to certain techniques. This simply indicates immunity to both biologic and robitic techniques.
    #[serde(alias = "biologic")]
    Biologic = 0x10,
    #[serde(alias = "robotic")]
    Robotic = 0x20,
    #[serde(alias = "boss")]
    Boss = 0x40, // Possessed by Dark Falz, Motherbrain, Neifirst (both occurrences) and Army Eye. Conveys immunity to certain techs, possibly other effects.
    #[serde(alias = "superboss", alias = "super_boss")]
    SuperBoss = 0x80, // The name is just a guess. Only Dark Falz and Motherbrain appear to have the bit for this set. No idea what it does. May provide immunity to some things or have other effects.
    #[serde(alias = "unknowna", alias = "unknown_a")]
    UnknownA = 0x01,
    #[serde(alias = "unknownb", alias = "unknown_b")]
    UnknownB = 0x02,
    #[serde(alias = "unknownc", alias = "unknown_c")]
    UnknownC = 0x04,
    #[serde(alias = "unknownd", alias = "unknown_d")]
    UnknownD = 0x08,
}

impl EnemyType {
    fn from_byte(byte: u8) -> Box<[Self]> {
        let mut variants = Vec::with_capacity(8);
        for variant in Self::iter() {
            if variant == Self::default() {
                continue;
            }
            if byte & variant as u8 == variant as u8 {
                variants.push(variant);
            }
        }
        if byte & (Self::Biologic as u8 | Self::Robotic as u8) == 0 {
            variants.push(Self::default());
        }
        variants.into_boxed_slice()
    }

    fn to_byte(value: &[Self]) -> u8 {
        let mut byte = 0;
        for variant in value.iter().copied() {
            byte |= variant as u8;
        }
        byte
    }
}

#[repr(u8)]
#[derive(EnumIter, Serialize, Deserialize, Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Debug)]
pub enum SpellElemental {
    #[serde(alias = "fire")]
    Fire = 0x01,
    #[serde(alias = "ice")]
    Ice = 0x02,
    #[serde(alias = "air")]
    Air = 0x04,
    #[serde(alias = "lightning")]
    Lightning = 0x08,
}

impl SpellElemental {
    fn from_byte(mut byte: u8) -> Box<[Self]> {
        byte &= 0x0f;
        let mut variants = Vec::with_capacity(8);
        for variant in Self::iter() {
            if byte & variant as u8 == variant as u8 {
                variants.push(variant);
            }
        }
        variants.into_boxed_slice()
    }

    fn to_byte(value: &[Self]) -> u8 {
        let mut byte = 0;
        for variant in value.iter().copied() {
            byte |= variant as u8;
        }
        byte
    }
}

#[repr(u8)]
#[derive(EnumIter, Serialize, Deserialize, Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Debug)]
pub enum Enchant {
    #[serde(alias = "fire")]
    Fire = 0x01,
    #[serde(alias = "ice")]
    Ice = 0x02,
    #[serde(alias = "lightning")]
    Lightning = 0x04,
    #[serde(alias = "air")]
    Air = 0x08,
    #[serde(alias = "paralysis")]
    Paralysis = 0x10,
    #[serde(alias = "unknownb", alias = "unknown_b")]
    UnknownB = 0x20,
    #[serde(alias = "unknownc", alias = "unknown_c")]
    UnknownC = 0x40,
    #[serde(alias = "unknownd", alias = "unknown_d")]
    UnknownD = 0x80,
}

impl Enchant {
    fn from_byte(byte: u8) -> Box<[Self]> {
        let mut variants = Vec::with_capacity(8);
        for variant in Self::iter() {
            if byte & variant as u8 == variant as u8 {
                variants.push(variant);
            }
        }
        variants.into_boxed_slice()
    }

    fn to_byte(value: &[Self]) -> u8 {
        let mut byte = 0;
        for variant in value.iter().copied() {
            byte |= variant as u8;
        }
        byte
    }
}

#[inline]
pub async fn parse_exec<P: AsRef<Path> + Send + Sync>(
    elf_exec: &PathBuf,
    out_dir: &P,
) -> Result<(), io::Error> {
    let elf_file = fs::File::open(elf_exec).await?;
    let mut elf_reader = BufReader::new(elf_file);

    // find_pointers(&mut elf_reader).await?;

    let mut pointers = BTreeMap::new();
    let exec_structures = ExecStructures {
        mapnames: parse_jumplist_strings(
            &mut elf_reader,
            MAPNAMES_JUMPLIST_START,
            MAPNAMES_JUMPLIST_FIELDS,
            &mut pointers,
        )
        .await?,
        songs: parse_songs(&mut elf_reader, &mut pointers).await?,
        items: items::parse(&mut elf_reader, &mut pointers).await?,
        misc_strings: parse_jumplist_strings(
            &mut elf_reader,
            MISC_STRINGS_JUMPLIST_START,
            MISC_STRINGS_JUMPLIST_FIELDS,
            &mut pointers,
        )
        .await?,
        techniques: techniques::parse(&mut elf_reader, &mut pointers).await?,
        enemies: enemies::parse(&mut elf_reader, &mut pointers).await?,
        menu_text: parse_jumplist_strings(
            &mut elf_reader,
            MENU_TEXT_JUMPLIST_START,
            MENU_TEXT_JUMPLIST_FIELDS,
            &mut pointers,
        )
        .await?,
        item_descriptions: parse_jumplist_strings(
            &mut elf_reader,
            ITEM_DESCRIPTION_JUMPLIST_START,
            ITEM_DESCRIPTION_JUMPLIST_FIELDS,
            &mut pointers,
        )
        .await?,
        savexit: parse_jumplist_strings(
            &mut elf_reader,
            SAVEXIT_JUMPLIST_START,
            SAVEXIT_JUMPLIST_FIELDS,
            &mut pointers,
        )
        .await?,
        question: parse_jumplist_strings(
            &mut elf_reader,
            QUESTION_JUMPLIST_START,
            QUESTION_JUMPLIST_FIELDS,
            &mut pointers,
        )
        .await?,
        // dunno: parse_jumplist_strings(
        //     &mut elf_reader,
        //     DUNNO_JUMPLIST_START,
        //     DUNNO_JUMPLIST_FIELDS,
        //     &mut aliaser,
        // )
        // .await?,
        // indicators: parse_jumplist_strings(
        //     &mut elf_reader,
        //     INDICATORS_JUMPLIST_START,
        //     INDICATORS_JUMPLIST_FIELDS,
        //     &mut aliaser,
        // )
        // .await?,
        memcard_opts: parse_memcard_opts(&mut elf_reader, &mut pointers).await?,
        end_credits: end_credits::parse(&mut elf_reader).await?,
        // dunno_struct: parse_structs(
        //     &mut elf_reader,
        //     DUNNO_STRUCT_START,
        //     DUNNO_STRUCT_FIELDS,
        //     DUNNO_STRUCT_COUNT,
        //     1,
        // )
        // .await?,
        other: parse_jumpless(&mut elf_reader).await?,
    };
    let exec_structures_path = PathBuf::with_capacity(128)
        .join(out_dir)
        .join(EXEC_STRUCTURES_FILENAME);
    save_binary_file(
        &exec_structures_path,
        toml::to_string_pretty(&exec_structures).unwrap().as_bytes(),
    )
    .await?;
    Ok(())
}

// async fn find_pointers(elf_reader: &mut BufReader<fs::File>) -> Result<(), io::Error> {
//     let mut pointer_candidates = IndexMap::with_capacity(10240);
//     let mut string_alias_candidates = HashSet::with_capacity(128);
//     let mut mem_unit = [0u8; 4];
//     elf_reader.seek(SeekFrom::Start(0)).await?;
//     let mut pointer_offset = 0;
//     while elf_reader.read_exact(&mut mem_unit).await.is_ok() {
//         let maybe_pointer =
//             u32::from_le_bytes(mem_unit).saturating_sub(u32::try_from(POINTER_OFFSET).unwrap());
//         if maybe_pointer > u32::try_from(POINTER_OFFSET).unwrap() && maybe_pointer < 0x1D_BC10 {
//             let pointer_locations = pointer_candidates
//                 .entry(maybe_pointer)
//                 .or_insert(Vec::with_capacity(3));
//             pointer_locations.push(pointer_offset);
//             if pointer_locations.len() > 1 {
//                 string_alias_candidates.insert(maybe_pointer);
//             }
//         }
//         pointer_offset = u32::try_from(elf_reader.stream_position().await.unwrap()).unwrap();
//     }
//     let mut string_pointers = BTreeMap::new();
//     for (offset, alias_pointers) in &pointer_candidates {
//         if let Ok(region) = MemRegion::try_from(*offset) {
//             println!(
//                 "{region:?}: {} -- {}",
//                 encode_hex(&offset.to_be_bytes()),
//                 alias_pointers
//                     .iter()
//                     .map(|p| encode_hex(&p.to_be_bytes()))
//                     .join(", ")
//             );
//             string_pointers.insert(*offset, alias_pointers.clone());
//         }
//     }
//     println!("\n\nPossible aliased strings:\n");
//     Ok(for (offset, pointers) in &pointer_candidates {
//         if MemRegion::try_from(*offset).is_ok() && string_alias_candidates.contains(offset) {
//             print!("0x{}: ", encode_hex(&offset.to_be_bytes()));
//             for pointer in pointers {
//                 print!("{}, ", encode_hex(&pointer.to_be_bytes()));
//             }
//             println!();
//         }
//     })
// }

#[inline]
#[expect(clippy::shadow_reuse, reason = "Easier to read")]
pub async fn patch_exec(dest: &PathBuf, exec_data_path: PathBuf) -> Result<(), io::Error> {
    if log_enabled!(Level::Info) {
        info!("Patching '{}'", dest.to_string_lossy());
    }
    let ExecStructures {
        mapnames,
        songs,
        items,
        misc_strings,
        memcard_opts,
        techniques,
        enemies,
        menu_text,
        item_descriptions,
        end_credits,
        savexit: _a,
        other: _b,
        question: _c, // indicators: _b,
                      // dunno: _c,
    } = load_exec_struct_patch(exec_data_path)?;
    unset_readonly(dest).await?;

    // To maintain original string layout:
    // items before misc
    // menus before item desc
    // mapnames before songs
    // techniques before enemies
    let mut interner = HashMap::with_capacity(1024);
    // Sort items by relative string pointer
    let mut region_buckets: BTreeMap<MemRegion, Vec<u8>> = BTreeMap::new();

    let mapnames = fill_mem_region(mapnames, &mut interner, &mut region_buckets);
    let songs = fill_mem_region(songs, &mut interner, &mut region_buckets);
    let items = fill_mem_region(items, &mut interner, &mut region_buckets);
    let misc_strings = fill_mem_region(misc_strings, &mut interner, &mut region_buckets);
    let memcard_opts = fill_mem_region(memcard_opts, &mut interner, &mut region_buckets);
    let techniques = fill_mem_region(techniques, &mut interner, &mut region_buckets);
    let enemies = fill_mem_region(enemies, &mut interner, &mut region_buckets);
    let menu_text = fill_mem_region(menu_text, &mut interner, &mut region_buckets);
    let item_descriptions = fill_mem_region(item_descriptions, &mut interner, &mut region_buckets);

    // for (region, bytes) in region_buckets {
    //     println!("{region:?}: {}\n", String::from_utf8_lossy(&bytes))
    // }

    let elf_binary = OpenOptions::new().write(true).open(dest).await?;
    let mut bw = BufWriter::new(elf_binary);
    patch_jumplist(
        &mut bw,
        mapnames,
        MAPNAMES_JUMPLIST_START,
        MAPNAMES_JUMPLIST_FIELDS,
    )
    .await?;
    patch_songs(&mut bw, songs).await?;
    items::patch(&mut bw, items).await?;
    patch_jumplist(
        &mut bw,
        misc_strings,
        MISC_STRINGS_JUMPLIST_START,
        MISC_STRINGS_JUMPLIST_FIELDS,
    )
    .await?;
    patch_memcard_opts(&mut bw, memcard_opts).await?;
    techniques::patch(&mut bw, techniques).await?;
    enemies::patch(&mut bw, enemies).await?;
    patch_jumplist(
        &mut bw,
        menu_text,
        MENU_TEXT_JUMPLIST_START,
        MENU_TEXT_JUMPLIST_FIELDS,
    )
    .await?;
    patch_jumplist(
        &mut bw,
        item_descriptions,
        ITEM_DESCRIPTION_JUMPLIST_START,
        ITEM_DESCRIPTION_JUMPLIST_FIELDS,
    )
    .await?;
    end_credits::patch(&mut bw, end_credits).await?;
    bw.flush().await?;

    Ok(())
}

#[inline]
fn fill_mem_region<T: StringFill>(
    items: BTreeMap<Hexu32, T>,
    interner: &mut HashMap<(MemRegion, usize), usize>,
    region_buckets: &mut BTreeMap<MemRegion, Vec<u8>>,
) -> BTreeMap<Hexu32, T> {
    let sorted_by_ptr_address = items.into_iter().sorted_by(|(_, item_a), (_, item_b)| {
        Ord::cmp(
            &item_a.get_relative_pointer().position,
            &item_b.get_relative_pointer().position,
        )
    });
    let mut debug_buckets = BTreeMap::new();
    let mut updated_items = BTreeMap::new();
    // let mut relative_pointers = BTreeMap::new();
    for (num, mut item) in sorted_by_ptr_address {
        item.convert_text();
        item.pad_text(if *crate::ENGRISH.get().unwrap() { 1 } else { 8 });
        let text = item.get_text();
        let relative_pointer = item.get_relative_pointer();
        let region = relative_pointer.mem_region;
        let relative = relative_pointer.position;
        // Get the bytes we have for this region bucket, extend it with
        let region_bytes = region_buckets.entry(region).or_default();
        debug_buckets
            .entry(region)
            .or_insert_with(|| text.to_string());
        let (location, max_size) = region.offset_size();
        let offset = region_bytes.len() + location;
        let mut new_region_size = region_bytes.len();
        if item.get_relative_pointer().alias.is_none() {
            new_region_size += text.byte_len();
        }
        if new_region_size <= max_size {
            let string = text.clone().to_string();
            println!(
                "Adding {string} to {region:?} relative 0x{:02x}",
                relative.0
            );
            let adjusted_offset =
                if let Some((memregion, position)) = item.get_relative_pointer().alias {
                    println!("This is pointer aliased, won't add to region: {string}");
                    interner
                        .get(&(memregion, usize::try_from(position.0).unwrap()))
                        .copied()
                        .unwrap()
                } else {
                    println!("This is not pointer aliased, adding to region: {string}");
                    region_bytes.extend(text.clone().into_bytes(Some(offset)));
                    let rp = item.get_relative_pointer();
                    interner.insert(
                        (rp.mem_region, usize::try_from(rp.position.0).unwrap()),
                        offset,
                    );
                    offset
                };
            let vma_pointer = u32::from_be_bytes(
                u32::try_from(adjusted_offset + POINTER_OFFSET)
                    .unwrap()
                    .to_le_bytes(),
            );
            println!("VMA pointer {}", encode_hex(&vma_pointer.to_le_bytes()));
            item.set_vma_pointer(vma_pointer);
            updated_items.insert(num, item);
        } else {
            for (disp_region, bytes) in region_buckets.iter() {
                error!("{disp_region:?}: {}\n", encode_hex(bytes));
            }
            error!(
                "Region {region:?} exceeded length. Expected: {max_size}, Got: {new_region_size}. Exceeded by {} bytes.\nBuckets: {debug_buckets:?}\nWould have added {text}",
                new_region_size - max_size
            );
        }
    }
    updated_items
}

#[derive(Serialize, Deserialize, PartialEq, PartialOrd, Eq, Ord, Hash, Copy, Clone, Default)]
pub struct Hexu32(
    #[serde(
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex"
    )]
    u32,
);

pub trait StringFill {
    fn get_relative_pointer(&self) -> RelativePointerInfo;
    fn get_text(&'_ self) -> &'_ DialogString;
    fn pad_text(&mut self, size: u8);
    fn set_vma_pointer(&mut self, ptr_le: u32);
    fn convert_text(&mut self);
}

#[derive(Serialize, Deserialize)]
pub struct JumplistItem {
    #[serde(flatten)]
    string: DialogString,
    #[serde(
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex"
    )]
    text_pointer: u32, // Pointer to the string
    #[serde(
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex"
    )]
    text_vma_pointer: u32, // Literal VMA pointer to the string
    #[serde(flatten)]
    relative_name_pointer: RelativePointerInfo,
}

impl StringFill for JumplistItem {
    fn get_relative_pointer(&self) -> RelativePointerInfo {
        self.relative_name_pointer
    }

    fn get_text(&'_ self) -> &'_ DialogString {
        &self.string
    }

    fn pad_text(&mut self, size: u8) {
        self.string.set_padding(size);
    }

    fn set_vma_pointer(&mut self, ptr_le: u32) {
        if self.text_vma_pointer != ptr_le {
            warn!(
                "Got {}, expected {}",
                encode_hex(&ptr_le.to_le_bytes()),
                encode_hex(&self.text_vma_pointer.to_le_bytes())
            );
        }
        self.text_vma_pointer = ptr_le;
    }

    fn convert_text(&mut self) {}
}

pub async fn parse_jumplist_strings<R: AsyncBufRead + AsyncSeek + Unpin>(
    reader: &mut R,
    location: usize,
    count: usize,
    pointers: &mut BTreeMap<u32, (MemRegion, Hexu32)>,
) -> Result<BTreeMap<Hexu32, JumplistItem>, io::Error> {
    reader.seek(SeekFrom::Start(location as u64)).await?;
    let mut pointer_bytes = [0u8; 4];
    let mut pointer_vec = Vec::with_capacity(count);
    let mut strings = BTreeMap::new();
    let mut relative_pointer_index = IndexSet::with_capacity(count);
    for string_no in 0..count {
        reader.read_exact(&mut pointer_bytes).await?;
        let pointer = u32::from_le_bytes(pointer_bytes);
        pointer_vec.push((string_no, pointer));
        relative_pointer_index.insert(pointer);
    }
    relative_pointer_index.sort_unstable();
    assert_eq!(
        pointer_vec.len(),
        count,
        "Number of string items must be EXACT!"
    );
    for (string_no, ptr) in pointer_vec {
        reader
            .seek(SeekFrom::Start(u64::from(ptr) - POINTER_OFFSET as u64))
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
        let text_pointer = ptr - u32::try_from(POINTER_OFFSET).unwrap();
        let region = MemRegion::try_from(text_pointer).unwrap_or_default();
        let index = relative_pointer_index.get_index_of(&ptr).unwrap();
        let position = Hexu32(u32::try_from(index).unwrap());
        strings.insert(
            Hexu32(u32::try_from(string_no).unwrap()),
            JumplistItem {
                string: engrish_str,
                text_pointer,
                text_vma_pointer: u32::from_le_bytes(ptr.to_be_bytes()),
                relative_name_pointer: RelativePointerInfo::new(
                    region,
                    position,
                    pointers.get(&ptr).copied(),
                ),
            },
        );
        pointers.insert(ptr, (region, position));
    }
    Ok(strings)
}

#[inline]
pub async fn patch_jumplist(
    exec_writer: &mut BufWriter<fs::File>,
    jumplist_items: BTreeMap<Hexu32, JumplistItem>,
    location: usize,
    count: usize,
) -> Result<(), io::Error> {
    exec_writer.seek(SeekFrom::Start(location as u64)).await?;
    assert_eq!(count, jumplist_items.len(), "Jumplist count MUST be exact!");
    for (_, jumplist_item) in jumplist_items {
        // Now write it all
        exec_writer
            .write_all(&jumplist_item.text_vma_pointer.to_be_bytes())
            .await?;
    }
    Ok(())
}

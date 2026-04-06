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
    helpers::{deserialize_u32_hex, save_binary_file, serialize_u32_hex, unset_readonly},
    slpm_patcher::{
        end_credits::EndCreditItem, enemies::EnemyInfo, items::ItemInfo, techniques::Technique,
    },
};
use alloc::collections::BTreeMap;
use indexmap::IndexSet;
use log::{Level, info, log_enabled};
use serde::{Deserialize, Serialize};
use std::{
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

// static DUNNO_JUMPLIST_START: usize = 0x18_B048;
// static DUNNO_JUMPLIST_FIELDS: usize = 5;

// static STRING_REGION_A_START: usize = 0x1A_9AF0; // menu text
// static STRING_REGION_A_END: usize = 0x1A_D802; //

// static ENEMY_NAMES_STRING_REGION_START: usize = 0x1A_89E0;
// static ENEMY_NAMES_STRING_REGION_END: usize = 0x1A_929F;

static MEMCARD_STRUCT_START: usize = 0x18_E0A0;
static MEMCARD_STRUCT_COUNT: usize = 9;
static MEMCARD_STRUCT_FIELDS: usize = 2;

static MUSIC_STRUCT_START: usize = 0x18_9450;
static MUSIC_STRUCT_COUNT: usize = 19;
static MUSIC_STRUCT_FIELDS: usize = 2;

static TECHNIQUE_STRUCT_START: usize = 0x1A_28A0;
static TECHNIQUE_STRUCT_COUNT: usize = 83;
static TECHNIQUE_STRUCT_FIELDS: usize = 14;

// static DUNNO_STRUCT_START: usize = 0x18_E0A0; // end 1A3AC8
// static DUNNO_STRUCT_COUNT: usize = 9;
// static DUNNO_STRUCT_FIELDS: usize = 2;

#[derive(Serialize, Deserialize, Ord, PartialOrd, Eq, PartialEq, Copy, Clone)]
enum StringMemRegion {
    #[serde(alias="regiona", alias="region_a")]
    RegionA,
    #[serde(alias="regionb", alias="region_b")]
    RegionB,
    #[serde(alias="regionc", alias="region_c")]
    RegionC,
    #[serde(alias="regiond", alias="region_d")]
    RegionD,
    #[serde(alias="regione", alias="region_e")]
    RegionE,
    #[serde(alias="regionf", alias="region_f")]
    RegionF,
    #[serde(alias="regiong", alias="region_g")]
    RegionG,
    #[serde(alias="regionh", alias="region_h")]
    RegionH,
    #[serde(alias="regioni", alias="region_i")]
    RegionI,
    #[serde(alias="regionj", alias="region_j")]
    RegionJ,
    #[serde(alias="regiongoldenboy", alias="region_goldenboy")]
    RegionGoldenboy, // This memory region is possibly invalid...could cause bugs on a real PS2?
    #[serde(alias="regionk", alias="region_k")]
    RegionK,
    #[serde(alias="regionl", alias="region_l")]
    RegionL,
    #[serde(alias="regionm", alias="region_m")]
    RegionM,
}

impl StringMemRegion {
    const fn offset_size(self) -> (usize, usize) {
        match self {
            Self::RegionA => (0x18_8770, 0x940),
            Self::RegionB => (0x18_94E8, 0x1a8),
            Self::RegionC => (0x18_C8F0, 0x10e8),
            Self::RegionD => (0x18_DF18, 0x188),
            Self::RegionE => (0x1A_3AC8, 0x450),
            Self::RegionF => (0x1A_89E0, 0x8b8),
            Self::RegionG => (0x1A_9AF0, 0x3d18),
            Self::RegionH => (0x1B_1840, 0x100),
            Self::RegionI => (0x1D_B218, 0x18),
            Self::RegionJ => (0x1D_B2D8, 0x58),
            Self::RegionGoldenboy => (0x1D_B330, 0x8),
            Self::RegionK => (0x1D_B338, 0x150),
            Self::RegionL => (0x1D_B598, 0xa0),
            Self::RegionM => (0x1D_B680, 0x128),
        }
    }
}

impl TryFrom<u32> for StringMemRegion {
    type Error = String;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            0x18_8770..0x18_90B0 => Ok(Self::RegionA),
            0x18_94E8..0x18_9690 => Ok(Self::RegionB),
            0x18_C8F0..0x18_D9D8 => Ok(Self::RegionC),
            0x18_DF18..0x18_E0A0 => Ok(Self::RegionD),
            0x1A_3AC8..0x1A_3F18 => Ok(Self::RegionE),
            0x1A_89E0..0x1A_9298 => Ok(Self::RegionF),
            0x1A_9AF0..0x1A_D808 => Ok(Self::RegionG),
            0x1B_1840..0x1B_1940 => Ok(Self::RegionH),
            0x1D_B218..0x1D_B230 => Ok(Self::RegionI),
            0x1D_B2D8..0x1D_B330 => Ok(Self::RegionJ),
            0x1D_B330..0x1D_B338 => Ok(Self::RegionGoldenboy),
            0x1D_B338..0x1D_B488 => Ok(Self::RegionK),
            0x1D_B598..0x1D_B638 => Ok(Self::RegionL),
            0x1D_B680..0x1D_B7A8 => Ok(Self::RegionM),
            _ => Err(format!("No map region for address 0x{value:06x}")),
        }
    }
}

#[repr(u8)]
#[derive(EnumIter, Serialize, Deserialize, PartialEq, Copy, Clone)]
enum Character {
    #[serde(alias="eusis")]
    Eusis = 0x01,
    #[serde(alias="nei")]
    Nei = 0x02,
    #[serde(alias="rudger")]
    Rudger = 0x04,
    #[serde(alias="anne")]
    Anne = 0x08,
    #[serde(alias="huey")]
    Huey = 0x10,
    #[serde(alias="amia")]
    Amia = 0x20,
    #[serde(alias="keinz")]
    Keinz = 0x40,
    #[serde(alias="silka")]
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
    relative_name_pointer: (StringMemRegion, Hexu32),
    field_1: i32,
}

#[derive(Serialize, Deserialize)]
pub struct MemcardOpt {
    #[serde(
        deserialize_with = "deserialize_dialog_items",
        serialize_with = "serialize_dialog_items"
    )]
    text: Vec<DialogItem>,
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
    relative_name_pointer: (StringMemRegion, Hexu32),
    field_1: i32,
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
) -> Result<BTreeMap<Hexu32, Song>, io::Error> {
    reader
        .seek(SeekFrom::Start(MUSIC_STRUCT_START as u64))
        .await?;
    let mut field_bytes = [0u8; 4];
    let mut songs = BTreeMap::new();
    let mut relative_pointer_index = IndexSet::with_capacity(MUSIC_STRUCT_COUNT);
    for i in 0..MUSIC_STRUCT_COUNT {
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
            relative_name_pointer: (StringMemRegion::RegionA, Hexu32(0)),
            field_1: i32::from_le_bytes(fields.pop().unwrap()),
        };
        songs.insert(Hexu32(u32::try_from(i).unwrap()), song);
    }
    relative_pointer_index.sort_unstable();

    // let mut name_pointers = BTreeMap::new();

    for song in songs.values_mut() {
        reader
            .seek(SeekFrom::Start(u64::from(song.name_pointer)))
            .await?;
        let mut string_bytes = Vec::with_capacity(20);
        while let Ok(byte) = reader.read_u8().await
            && byte != 0
        {
            string_bytes.push(byte);
        }
        song.name = decode_psg2_string(string_bytes).text;
        let region = StringMemRegion::try_from(song.name_pointer).unwrap();
        let index = relative_pointer_index
            .get_index_of(&song.name_pointer)
            .unwrap();
        song.relative_name_pointer = (region, Hexu32(u32::try_from(index).unwrap()));
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
        exec_writer
            .write_all(&song.field_1.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&song.name_vma_pointer.to_be_bytes())
            .await?;
    }
    Ok(())
}

pub async fn parse_memcard_opts<R: AsyncBufRead + AsyncSeek + Unpin>(
    reader: &mut R,
) -> Result<BTreeMap<Hexu32, MemcardOpt>, io::Error> {
    reader
        .seek(SeekFrom::Start(MEMCARD_STRUCT_START as u64))
        .await?;
    let mut field_bytes = [0u8; 4];
    let mut memcard_opts = BTreeMap::new();
    let mut relative_pointer_index = IndexSet::with_capacity(MEMCARD_STRUCT_COUNT);
    for i in 0..MEMCARD_STRUCT_COUNT {
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
            text: Vec::new(),
            name_pointer,
            name_vma_pointer: u32::from_be_bytes(pointer_bytes),
            relative_name_pointer: (StringMemRegion::RegionA, Hexu32(0)),
            field_1: i32::from_le_bytes(fields.pop().unwrap()),
        };
        memcard_opts.insert(Hexu32(u32::try_from(i).unwrap()), memcard_opt);
    }
    relative_pointer_index.sort_unstable();

    // let mut name_pointers = BTreeMap::new();

    for memcard_opt in memcard_opts.values_mut() {
        reader
            .seek(SeekFrom::Start(u64::from(memcard_opt.name_pointer)))
            .await?;
        let mut string_bytes = Vec::with_capacity(20);
        while let Ok(byte) = reader.read_u8().await
            && byte != 0
        {
            string_bytes.push(byte);
        }
        memcard_opt.text = decode_psg2_string(string_bytes).text;
        let region = StringMemRegion::try_from(memcard_opt.name_pointer).unwrap();
        let index = relative_pointer_index
            .get_index_of(&memcard_opt.name_pointer)
            .unwrap();
        memcard_opt.relative_name_pointer = (region, Hexu32(u32::try_from(index).unwrap()));
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
            .write_all(&memcard_opt.field_1.to_le_bytes())
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
    // pub dunno: BTreeMap<Hexu32, DialogString>,
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
    #[serde(alias="demonic")]
    Demonic, // First and second bits turned off. Effectively, the below two bits count as a weakness to certain techniques. This simply indicates immunity to both biologic and robitic techniques.
    #[serde(alias="biologic")]
    Biologic = 0x10,
    #[serde(alias="robotic")]
    Robotic = 0x20,
    #[serde(alias="boss")]
    Boss = 0x40, // Possessed by Dark Falz, Motherbrain, Neifirst (both occurrences) and Army Eye. Conveys immunity to certain techs, possibly other effects.
    #[serde(alias="superboss", alias="super_boss")]
    SuperBoss = 0x80, // The name is just a guess. Only Dark Falz and Motherbrain appear to have the bit for this set. No idea what it does. May provide immunity to some things or have other effects.
    #[serde(alias="unknowna", alias="unknown_a")]
    UnknownA = 0x01,
    #[serde(alias="unknownb", alias="unknown_b")]
    UnknownB = 0x02,
    #[serde(alias="unknownc", alias="unknown_c")]
    UnknownC = 0x04,
    #[serde(alias="unknownd", alias="unknown_d")]
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
enum SpellElemental {
    #[serde(alias="fire")]
    Fire = 0x01,
    #[serde(alias="ice")]
    Ice = 0x02,
    #[serde(alias="air")]
    Air = 0x04,
    #[serde(alias="lightning")]
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
enum Enchant {
    #[serde(alias="fire")]
    Fire = 0x01,
    #[serde(alias="ice")]
    Ice = 0x02,
    #[serde(alias="lightning")]
    Lightning = 0x04,
    #[serde(alias="air")]
    Air = 0x08,
    #[serde(alias="paralysis")]
    Paralysis = 0x10,
    #[serde(alias="unknownb", alias="unknown_b")]
    UnknownB = 0x20,
    #[serde(alias="unknownc", alias="unknown_c")]
    UnknownC = 0x40,
    #[serde(alias="unknownd", alias="unknown_d")]
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
pub async fn generate_exec_data<P: AsRef<Path> + Send + Sync>(
    elf_exec: &PathBuf,
    out_dir: &P,
) -> Result<(), io::Error> {
    let elf_file = fs::File::open(elf_exec).await?;
    let mut elf_reader = BufReader::new(elf_file);
    let exec_structures = ExecStructures {
        mapnames: parse_jumplist_strings(
            &mut elf_reader,
            MAPNAMES_JUMPLIST_START,
            MAPNAMES_JUMPLIST_FIELDS,
        )
        .await?,
        songs: parse_songs(&mut elf_reader).await?,
        items: items::parse(&mut elf_reader).await?,
        misc_strings: parse_jumplist_strings(
            &mut elf_reader,
            MISC_STRINGS_JUMPLIST_START,
            MISC_STRINGS_JUMPLIST_FIELDS,
        )
        .await?,
        techniques: techniques::parse(&mut elf_reader).await?,
        enemies: enemies::parse(&mut elf_reader).await?,
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
        // dunno: parse_jumplist_strings(&mut elf_reader, DUNNO_JUMPLIST_START, DUNNO_JUMPLIST_FIELDS)
        //     .await?,
        memcard_opts: parse_memcard_opts(&mut elf_reader).await?,
        end_credits: end_credits::parse(&mut elf_reader).await?,
        // dunno_struct: parse_structs(
        //     &mut elf_reader,
        //     DUNNO_STRUCT_START,
        //     DUNNO_STRUCT_FIELDS,
        //     DUNNO_STRUCT_COUNT,
        //     1,
        // )
        // .await?,
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

#[inline]
pub async fn patch_exec(dest: &PathBuf, exec_data_path: PathBuf) -> Result<(), io::Error> {
    if log_enabled!(Level::Info) {
        info!("Patching '{}'", dest.to_string_lossy());
    }
    let ExecStructures {
        mapnames,
        menu_text,
        item_descriptions,
        misc_strings,
        // dunno: _e,
        // dunno_struct: _e,
        techniques,
        songs,
        memcard_opts,
        items,
        enemies,
        end_credits,
    } = load_exec_struct_patch(exec_data_path)?;
    unset_readonly(dest).await?;
    let elf_binary = OpenOptions::new().write(true).open(dest).await?;
    let mut bw = BufWriter::new(elf_binary);
    patch_jumplist(&mut bw, mapnames, MAPNAMES_JUMPLIST_START, MAPNAMES_JUMPLIST_FIELDS).await?;
    patch_songs(&mut bw, songs).await?;
    items::patch(&mut bw, items).await?;
    patch_jumplist(&mut bw, misc_strings, MISC_STRINGS_JUMPLIST_START, MISC_STRINGS_JUMPLIST_FIELDS).await?;
    techniques::patch(&mut bw, techniques).await?;
    enemies::patch(&mut bw, enemies).await?;
    patch_jumplist(&mut bw, menu_text, MENU_TEXT_JUMPLIST_START, MENU_TEXT_JUMPLIST_FIELDS).await?;
    patch_jumplist(&mut bw, item_descriptions, ITEM_DESCRIPTION_JUMPLIST_START, ITEM_DESCRIPTION_JUMPLIST_FIELDS).await?;
    end_credits::patch(&mut bw, end_credits).await?;
    patch_memcard_opts(&mut bw, memcard_opts).await?;
    bw.flush().await?;

    Ok(())
}

#[derive(Serialize, Deserialize, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub struct Hexu32(
    #[serde(
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex"
    )]
    u32,
);

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
    relative_name_pointer: (StringMemRegion, Hexu32),
}

pub async fn parse_jumplist_strings<R: AsyncBufRead + AsyncSeek + Unpin>(
    reader: &mut R,
    location: usize,
    count: usize,
) -> Result<BTreeMap<Hexu32, JumplistItem>, io::Error> {
    reader.seek(SeekFrom::Start(location as u64)).await?;
    let mut pointer_bytes = [0u8; 4];
    let mut pointer_vec = Vec::with_capacity(count);
    let mut strings = BTreeMap::new();
    let mut relative_pointer_index = IndexSet::with_capacity(count);
    for i in 0..count {
        reader.read_exact(&mut pointer_bytes).await?;
        let pointer = u32::from_le_bytes(pointer_bytes);
        pointer_vec.push((i, pointer));
        relative_pointer_index.insert(pointer);
    }
    relative_pointer_index.sort_unstable();
    assert_eq!(
        pointer_vec.len(),
        count,
        "Number of string items must be EXACT!"
    );
    for (number, pointer) in pointer_vec {
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
        let text_pointer = pointer - u32::try_from(POINTER_OFFSET).unwrap();
        let region = StringMemRegion::try_from(text_pointer).unwrap();
        let index = relative_pointer_index.get_index_of(&pointer).unwrap();
        strings.insert(
            Hexu32(u32::try_from(number).unwrap()),
            JumplistItem {
                string: engrish_str,
                text_pointer,
                text_vma_pointer: u32::from_le_bytes(pointer.to_be_bytes()),
                relative_name_pointer: (region, Hexu32(u32::try_from(index).unwrap())),
            },
        );
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
    exec_writer
        .seek(SeekFrom::Start(location as u64))
        .await?;
    assert_eq!(
        count,
        jumplist_items.len(),
        "Jumplist count MUST be exact!"
    );
    for (_, jumplist_item) in jumplist_items {
        // Now write it all
        exec_writer
            .write_all(&jumplist_item.text_vma_pointer.to_be_bytes())
            .await?;
    }
    Ok(())
}

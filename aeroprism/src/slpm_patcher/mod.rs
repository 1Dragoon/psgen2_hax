#![allow(clippy::arbitrary_source_item_ordering, reason = "not needed")]
pub mod end_credits;
pub mod enemies;
pub mod items;
pub mod techniques;
use crate::{
    EXEC_STRUCTURES_FILENAME,
    events::{DialogItem, codec::decode_psg2_string, load_exec_struct_patch},
    helpers::{save_binary_file, unset_readonly},
    slpm_patcher::{
        end_credits::EndCreditItem, enemies::EnemyInfo, items::ItemInfo, techniques::Technique,
    },
};
use alloc::collections::BTreeMap;
// use indexmap::IndexMap;
use crate::{
    events::{DialogString, deserialize_dialog_items, serialize_dialog_items},
    helpers::{deserialize_u32_hex, serialize_u32_hex},
};
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

static TECHNIQUE_STRUCT_START: usize = 0x1A_28A0;
static TECHNIQUE_STRUCT_COUNT: usize = 83;
static TECHNIQUE_STRUCT_FIELDS: usize = 14;

static MUSIC_STRUCT_START: usize = 0x18_9450;
static MUSIC_STRUCT_COUNT: usize = 19;
static MUSIC_STRUCT_FIELDS: usize = 2;

static MEMCARD_STRUCT_START: usize = 0x18_E0A0;
static MEMCARD_STRUCT_COUNT: usize = 9;
static MEMCARD_STRUCT_FIELDS: usize = 2;

// static DUNNO_STRUCT_START: usize = 0x18_E0A0; // end 1A3AC8
// static DUNNO_STRUCT_COUNT: usize = 9;
// static DUNNO_STRUCT_FIELDS: usize = 2;

#[repr(u8)]
#[derive(EnumIter, Serialize, Deserialize, PartialEq, Copy, Clone)]
enum Character {
    Eusis = 0x01,
    Nei = 0x02,
    Rudger = 0x04,
    Anne = 0x08,
    Huey = 0x10,
    Amia = 0x20,
    Keinz = 0x40,
    Silka = 0x80,
}

impl Character {
    pub fn character_list_from_byte(character_byte: u8) -> Box<[Self]> {
        let mut characters = Vec::with_capacity(8);
        for character in Self::iter() {
            if character_byte & (character as u8) == (character as u8) {
                characters.push(character);
            }
        }
        characters.into_boxed_slice()
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
    #[serde(
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex"
    )]
    field_1: u32,
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
    #[serde(
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex"
    )]
    field_1: u32,
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
    for i in 0..MUSIC_STRUCT_COUNT {
        let mut fields = Vec::with_capacity(MUSIC_STRUCT_FIELDS);
        for _ in 0..MUSIC_STRUCT_FIELDS {
            reader.read_exact(&mut field_bytes).await?;
            fields.push(field_bytes);
        }
        let pointer_bytes = fields.pop().unwrap();
        let song = Song {
            name: Vec::new(),
            name_pointer: u32::from_le_bytes(pointer_bytes)
                - u32::try_from(POINTER_OFFSET).unwrap(),
            name_vma_pointer: u32::from_be_bytes(pointer_bytes),
            field_1: u32::from_le_bytes(fields.pop().unwrap()),
        };
        songs.insert(Hexu32(u32::try_from(i).unwrap()), song);
    }

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

pub async fn parse_memcard_opts<R: AsyncBufRead + AsyncSeek + Unpin>(
    reader: &mut R,
) -> Result<BTreeMap<Hexu32, MemcardOpt>, io::Error> {
    reader
        .seek(SeekFrom::Start(MEMCARD_STRUCT_START as u64))
        .await?;
    let mut field_bytes = [0u8; 4];
    let mut memcard_opts = BTreeMap::new();
    for i in 0..MEMCARD_STRUCT_COUNT {
        let mut fields = Vec::with_capacity(MEMCARD_STRUCT_FIELDS);
        for _ in 0..MEMCARD_STRUCT_FIELDS {
            reader.read_exact(&mut field_bytes).await?;
            fields.push(field_bytes);
        }
        let pointer_bytes = fields.pop().unwrap();
        let memcard_opt = MemcardOpt {
            text: Vec::new(),
            name_pointer: u32::from_le_bytes(pointer_bytes)
                - u32::try_from(POINTER_OFFSET).unwrap(),
            name_vma_pointer: u32::from_be_bytes(pointer_bytes),
            field_1: u32::from_le_bytes(fields.pop().unwrap()),
        };
        memcard_opts.insert(Hexu32(u32::try_from(i).unwrap()), memcard_opt);
    }

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

#[derive(Serialize, Deserialize)]
pub struct ExecStructures {
    // pub dunno_struct: indexmap::IndexMap<Hexu32, (DialogString, Vec<Hexu32>)>,
    #[serde(rename = "memcard_opt")]
    pub memcard_opts: BTreeMap<Hexu32, MemcardOpt>,
    #[serde(rename = "technique")]
    pub techniques: BTreeMap<Hexu32, Technique>,
    #[serde(rename = "song")]
    pub songs: BTreeMap<Hexu32, Song>,
    #[serde(rename = "item")]
    pub items: BTreeMap<Hexu32, ItemInfo>,
    #[serde(rename = "enemy")]
    pub enemies: BTreeMap<Hexu32, EnemyInfo>,
    #[serde(rename = "end_credit")]
    pub end_credits: BTreeMap<usize, EndCreditItem>,
    #[serde(rename = "mapname")]
    pub mapnames: BTreeMap<Hexu32, JumplistString>,
    pub menu_text: BTreeMap<Hexu32, JumplistString>,
    #[serde(rename = "item_description")]
    pub item_descriptions: BTreeMap<Hexu32, JumplistString>,
    #[serde(rename = "misc_string")]
    pub misc_strings: BTreeMap<Hexu32, JumplistString>,
    // pub dunno: BTreeMap<Hexu32, DialogString>,
}

#[repr(u8)]
#[derive(Serialize, Deserialize, Default, Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Debug)]
enum EnemyType {
    #[default]
    Demonic, // First and second bits turned off. Effectively, the below two bits count as a weakness to certain techniques. This simply indicates immunity to both biologic and robitic techniques.
    Biologic = 0x01,
    Robotic = 0x02,
}

#[repr(u8)]
#[derive(EnumIter, Serialize, Deserialize, Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Debug)]
enum SpellElemental {
    Fire = 0x01,
    Ice = 0x02,
    Air = 0x04,
    Lightning = 0x08,
}

impl SpellElemental {
    pub fn multi_from_byte(byte: u8) -> Box<[Self]> {
        let mut elementals = Vec::with_capacity(4);
        for ele in Self::iter() {
            if byte & ele as u8 == ele as u8 {
                elementals.push(ele);
            }
        }
        elementals.into_boxed_slice()
    }
}

#[repr(u8)]
#[derive(EnumIter, Serialize, Deserialize, Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Debug)]
enum ItemElemental {
    Fire = 0x01,
    Ice = 0x02,
    Lightning = 0x04,
    Air = 0x08,
}

impl ItemElemental {
    pub fn multi_from_byte(byte: u8) -> Box<[Self]> {
        let mut elementals = Vec::with_capacity(4);
        for ele in Self::iter() {
            if byte & ele as u8 == ele as u8 {
                elementals.push(ele);
            }
        }
        elementals.into_boxed_slice()
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
        misc_strings: parse_jumplist_strings(
            &mut elf_reader,
            MISC_STRINGS_JUMPLIST_START,
            MISC_STRINGS_JUMPLIST_FIELDS,
        )
        .await?,
        // dunno: parse_jumplist_strings(&mut elf_reader, DUNNO_JUMPLIST_START, DUNNO_JUMPLIST_FIELDS)
        //     .await?,
        memcard_opts: parse_memcard_opts(&mut elf_reader).await?,
        techniques: techniques::parse(&mut elf_reader).await?,
        songs: parse_songs(&mut elf_reader).await?,
        items: items::parse(&mut elf_reader).await?,
        enemies: enemies::parse(&mut elf_reader).await?,
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
        mapnames: _a,
        menu_text: _b,
        item_descriptions: _c,
        misc_strings: _d,
        // dunno: _e,
        // dunno_struct: _e,
        techniques: _f,
        songs: _g,
        memcard_opts: _h,
        items,
        enemies,
        end_credits,
    } = load_exec_struct_patch(exec_data_path)?;
    unset_readonly(dest).await?;
    let elf_binary = OpenOptions::new().write(true).open(dest).await?;
    let mut bw = BufWriter::new(elf_binary);
    items::patch(&mut bw, items).await?;
    enemies::patch(&mut bw, enemies).await?;
    end_credits::patch(&mut bw, end_credits).await?;
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
pub struct JumplistString {
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
}

pub async fn parse_jumplist_strings<R: AsyncBufRead + AsyncSeek + Unpin>(
    reader: &mut R,
    location: usize,
    count: usize,
) -> Result<BTreeMap<Hexu32, JumplistString>, io::Error> {
    reader.seek(SeekFrom::Start(location as u64)).await?;
    let mut pointer_bytes = [0u8; 4];
    let mut pointer_vec = Vec::with_capacity(count);
    let mut strings = BTreeMap::new();
    for i in 0..count {
        reader.read_exact(&mut pointer_bytes).await?;
        pointer_vec.push((i, u32::from_le_bytes(pointer_bytes)));
    }
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
        strings.insert(
            Hexu32(u32::try_from(number).unwrap()),
            JumplistString {
                string: engrish_str,
                text_pointer: pointer - u32::try_from(POINTER_OFFSET).unwrap(),
                text_vma_pointer: u32::from_le_bytes(pointer.to_be_bytes()),
            },
        );
    }
    Ok(strings)
}

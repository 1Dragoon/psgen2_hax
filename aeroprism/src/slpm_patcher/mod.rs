pub mod end_credits;
pub mod enemies;
pub mod items;
pub mod jumplists;
pub mod memcard_opts;
pub mod songs;
pub mod techniques;
use crate::{
    EXEC_STRUCTURES_FILENAME,
    events::{DialogString, codec::decode_psg2_string, load_exec_struct_patch},
    helpers::{Hexu32, encode_hex, is_default, save_binary_file, unset_readonly},
    slpm_patcher::{
        end_credits::EndCreditItem, enemies::EnemyInfo, items::ItemInfo, jumplists::JumplistItem,
        memcard_opts::MemcardOpt, songs::Song, techniques::Technique,
    },
};
use alloc::collections::BTreeMap;
use itertools::Itertools;
use log::{Level, debug, error, info, log_enabled};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    io,
    path::{Path, PathBuf},
};
use tokio::{
    fs::{self, OpenOptions},
    io::{
        AsyncBufRead, AsyncReadExt, AsyncSeek, AsyncSeekExt, AsyncWriteExt, BufReader, BufWriter,
        SeekFrom,
    },
};

// Via `readelf d:\SLPM_625.53 -l`
// Program segment 1 and 2 both end up as 0xFF000
static VMA_OFFSET: u32 = 0x10_0000;
static FILE_OFFSET: u32 = 0x1000;
pub static POINTER_OFFSET: u32 = VMA_OFFSET - FILE_OFFSET;

static MAPNAMES_JUMPLIST_START: usize = 0x14_F798;
static MAPNAMES_JUMPLIST_FIELDS: usize = 107;

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

// static DUNNO_STRUCT_START: usize = 0x1A4198; // end 1A3AC8
// static DUNNO_STRUCT_COUNT: usize = 9;
// static DUNNO_STRUCT_FIELDS: usize = 25;

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

#[expect(
    clippy::arbitrary_source_item_ordering,
    reason = "Ordered by address space location."
)]
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
    #[serde(rename = "sn")]
    StructuredN,
}

impl MemRegion {
    const fn offset_size(self) -> (usize, usize) {
        match self {
            Self::JumplessA => (0x18_7880, 0x50),
            Self::StructuredA => (0x18_8770, 0x940),
            Self::JumplessB => (0x18_9210, 0x90),
            Self::JumplessC => (0x18_92C8, 0x40),
            Self::StructuredB => (0x18_94E8, 0x1a8),
            Self::JumplessD => (0x18_96E8, 0x14), // Extended by 4 bytes for compatibility with goldenboy release
            Self::JumplessE => (0x18_A330, 0x420),
            Self::JumplessF => (0x18_A760, 0x48),
            Self::JumplessG => (0x18_B0A0, 0x30),
            Self::JumplessH => (0x18_B160, 0x18),
            Self::StructuredC => (0x18_C8F0, 0x10e8),
            Self::JumplessI => (0x18_DA10, 0x378),
            Self::StructuredD => (0x18_DF18, 0x188),
            Self::JumplessJ => (0x1A_1C80, 0x4c), // Extended by 4 bytes for compatibility with goldenboy release
            Self::JumplessK => (0x1A_1E78, 0x470),
            Self::StructuredE => (0x1A_1D80, 0x40),
            Self::StructuredF => (0x1A_3AC8, 0x450),
            Self::StructuredG => (0x1A_89E0, 0x8c0),
            Self::StructuredH => (0x1A_9AF0, 0x3d18),
            Self::StructuredI => (0x1B_1840, 0x100),
            Self::StructuredJ => (0x1D_B218, 0x18),
            Self::StructuredK => (0x1D_B238, 0x98),
            Self::StructuredL => (0x1D_B2D8, 0x1B0),
            Self::StructuredM => (0x1D_B598, 0xb0),
            Self::StructuredN => (0x1D_B680, 0x128),
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
            0x1A_1D80..0x1A_1DC0 => Ok(Self::StructuredE),
            0x1A_1E78..0x1A_22E8 => Ok(Self::JumplessK),
            0x1A_3AC8..0x1A_3F18 => Ok(Self::StructuredF),
            0x1A_89E0..0x1A_9298 => Ok(Self::StructuredG),
            0x1A_9AF0..0x1A_D808 => Ok(Self::StructuredH),
            0x1B_1840..0x1B_1940 => Ok(Self::StructuredI),
            0x1D_B218..0x1D_B230 => Ok(Self::StructuredJ),
            0x1D_B238..0x1D_B2D0 => Ok(Self::StructuredK),
            0x1D_B2D8..0x1D_B488 => Ok(Self::StructuredL),
            0x1D_B598..0x1D_B648 => Ok(Self::StructuredM),
            0x1D_B680..0x1D_B7A8 => Ok(Self::StructuredN),
            _ => Err(format!(
                "No defined memory region for address 0x{value:06x}"
            )),
        }
    }
}

#[expect(
    clippy::struct_field_names,
    reason = "This struct gets flattened in the output, where the names help."
)]
#[derive(Serialize, Deserialize, Default, Copy, Clone)]
pub struct RelativePointerInfo {
    #[serde(default, skip_serializing_if = "is_default")]
    pub string_aliased: bool,
    #[serde(default, skip_serializing_if = "is_default")]
    pub string_mem_region: MemRegion,
    #[serde(default, skip_serializing_if = "is_default")]
    pub string_position: Hexu32,
}

#[derive(Serialize, Deserialize)]
pub struct ExecStructures {
    // pub dunno_struct: indexmap::IndexMap<Hexu32, (DialogString, Vec<Hexu32>)>,
    #[serde(rename = "end_credit")]
    pub end_credits: BTreeMap<usize, EndCreditItem>,
    #[serde(rename = "enemy")]
    pub enemies: BTreeMap<Hexu32, EnemyInfo>,
    #[serde(rename = "item_description")]
    pub item_descriptions: BTreeMap<Hexu32, JumplistItem>,
    #[serde(rename = "item")]
    pub items: BTreeMap<Hexu32, ItemInfo>,
    pub jumpless_strings: BTreeMap<MemRegion, Vec<DialogString>>,
    #[serde(rename = "mapname")]
    pub mapnames: BTreeMap<Hexu32, JumplistItem>,
    #[serde(rename = "memcard_opt")]
    pub memcard_opts: BTreeMap<Hexu32, MemcardOpt>,
    pub menu_text: BTreeMap<Hexu32, JumplistItem>,
    #[serde(rename = "misc_string")]
    pub misc_strings: BTreeMap<Hexu32, JumplistItem>,
    pub question: BTreeMap<Hexu32, JumplistItem>,
    pub savexit: BTreeMap<Hexu32, JumplistItem>,
    #[serde(rename = "song")]
    pub songs: BTreeMap<Hexu32, Song>,
    #[serde(rename = "technique")]
    pub techniques: BTreeMap<Hexu32, Technique>,
    // pub indicators: BTreeMap<Hexu32, JumplistItem>,
    // pub dunno: BTreeMap<Hexu32, JumplistItem>,
}

pub trait StringFill {
    fn convert_text(&mut self);
    fn get_relative_pointer(&self) -> RelativePointerInfo;
    fn get_text(&'_ self) -> &'_ DialogString;
    fn pad_text(&mut self, size: u8);
    fn set_vma_pointer(&mut self, ptr_le: u32);
}

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
            let (terminator_pos, _) = region_bytes.iter().find_position(|b| **b == 0).unwrap(); // Can fail here if strings are added past the 64-bit boundary into the null field that comes after
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

#[inline]
pub async fn parse_exec<P: AsRef<Path> + Send + Sync>(
    elf_exec: &PathBuf,
    out_dir: &P,
) -> Result<(), io::Error> {
    let elf_file = fs::File::open(elf_exec).await?;
    let mut elf_reader = BufReader::new(elf_file);

    // find_pointers(&mut elf_reader).await?;

    let mut pointers = Vec::with_capacity(10240);
    let mut exec_structures = ExecStructures {
        mapnames: jumplists::parse_strings(
            &mut elf_reader,
            MAPNAMES_JUMPLIST_START,
            MAPNAMES_JUMPLIST_FIELDS,
            &mut pointers,
        )
        .await?,
        songs: songs::parse(&mut elf_reader, &mut pointers).await?,
        items: items::parse(&mut elf_reader, &mut pointers).await?,
        misc_strings: jumplists::parse_strings(
            &mut elf_reader,
            MISC_STRINGS_JUMPLIST_START,
            MISC_STRINGS_JUMPLIST_FIELDS,
            &mut pointers,
        )
        .await?,
        techniques: techniques::parse(&mut elf_reader, &mut pointers).await?,
        enemies: enemies::parse(&mut elf_reader, &mut pointers).await?,
        menu_text: jumplists::parse_strings(
            &mut elf_reader,
            MENU_TEXT_JUMPLIST_START,
            MENU_TEXT_JUMPLIST_FIELDS,
            &mut pointers,
        )
        .await?,
        item_descriptions: jumplists::parse_strings(
            &mut elf_reader,
            ITEM_DESCRIPTION_JUMPLIST_START,
            ITEM_DESCRIPTION_JUMPLIST_FIELDS,
            &mut pointers,
        )
        .await?,
        savexit: jumplists::parse_strings(
            &mut elf_reader,
            SAVEXIT_JUMPLIST_START,
            SAVEXIT_JUMPLIST_FIELDS,
            &mut pointers,
        )
        .await?,
        question: jumplists::parse_strings(
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
        memcard_opts: memcard_opts::parse(&mut elf_reader, &mut pointers).await?,
        end_credits: end_credits::parse(&mut elf_reader).await?,
        // dunno_struct: parse_structs(
        //     &mut elf_reader,
        //     DUNNO_STRUCT_START,
        //     DUNNO_STRUCT_FIELDS,
        //     DUNNO_STRUCT_COUNT,
        //     1,
        // )
        // .await?,
        jumpless_strings: parse_jumpless(&mut elf_reader).await?,
    };

    pointers.sort_unstable();
    pointers.dedup();
    let mut relatives = HashMap::with_capacity(10240);
    for (position, pointer) in pointers.into_iter().enumerate() {
        let relative = RelativePointerInfo {
            string_mem_region: MemRegion::try_from(pointer - POINTER_OFFSET).unwrap(),
            string_position: Hexu32(u32::try_from(position).unwrap()),
            string_aliased: false,
        };
        relatives.insert(pointer, relative);
    }

    let mut aliases = HashSet::with_capacity(64);
    for item in exec_structures.techniques.values_mut() {
        let ptr = item.name_vma_pointer;
        let mut rnp = relatives.get(&ptr).copied().unwrap();
        rnp.string_aliased = !aliases.insert(ptr);
        item.relative_name_pointer = rnp;
    }
    for item in exec_structures.items.values_mut() {
        let ptr = item.name_vma_pointer;
        let mut rnp = relatives.get(&ptr).copied().unwrap();
        rnp.string_aliased = !aliases.insert(ptr);
        item.relative_name_pointer = rnp;
    }
    for item in exec_structures.enemies.values_mut() {
        let ptr = item.name_vma_pointer;
        let mut rnp = relatives.get(&ptr).copied().unwrap();
        rnp.string_aliased = !aliases.insert(ptr);
        item.relative_name_pointer = rnp;
    }
    for item in exec_structures.mapnames.values_mut() {
        let ptr = item.text_vma_pointer;
        let mut rnp = relatives.get(&ptr).copied().unwrap();
        rnp.string_aliased = !aliases.insert(ptr);
        item.relative_name_pointer = rnp;
    }
    for item in exec_structures.menu_text.values_mut() {
        let ptr = item.text_vma_pointer;
        let mut rnp = relatives.get(&ptr).copied().unwrap();
        rnp.string_aliased = !aliases.insert(ptr);
        item.relative_name_pointer = rnp;
    }
    for item in exec_structures.item_descriptions.values_mut() {
        let ptr = item.text_vma_pointer;
        let mut rnp = relatives.get(&ptr).copied().unwrap();
        rnp.string_aliased = !aliases.insert(ptr);
        item.relative_name_pointer = rnp;
    }
    for item in exec_structures.songs.values_mut() {
        let ptr = item.name_vma_pointer;
        let mut rnp = relatives.get(&ptr).copied().unwrap();
        rnp.string_aliased = !aliases.insert(ptr);
        item.relative_name_pointer = rnp;
    }
    for item in exec_structures.misc_strings.values_mut() {
        let ptr = item.text_vma_pointer;
        let mut rnp = relatives.get(&ptr).copied().unwrap();
        rnp.string_aliased = !aliases.insert(ptr);
        item.relative_name_pointer = rnp;
    }
    for item in exec_structures.savexit.values_mut() {
        let ptr = item.text_vma_pointer;
        let mut rnp = relatives.get(&ptr).copied().unwrap();
        rnp.string_aliased = !aliases.insert(ptr);
        item.relative_name_pointer = rnp;
    }
    for item in exec_structures.question.values_mut() {
        let ptr = item.text_vma_pointer;
        let mut rnp = relatives.get(&ptr).copied().unwrap();
        rnp.string_aliased = !aliases.insert(ptr);
        item.relative_name_pointer = rnp;
    }
    for item in exec_structures.memcard_opts.values_mut() {
        let ptr = item.name_vma_pointer;
        let mut rnp = relatives.get(&ptr).copied().unwrap();
        rnp.string_aliased = !aliases.insert(ptr);
        item.relative_name_pointer = rnp;
    }

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
        savexit,
        question,
        // indicators: _a,
        // dunno: _b,
        jumpless_strings,
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
    let savexit = fill_mem_region(savexit, &mut interner, &mut region_buckets);
    let question = fill_mem_region(question, &mut interner, &mut region_buckets);

    for (region, strings) in jumpless_strings {
        for mut string in strings {
            string.set_padding(if *crate::ENGRISH.get().unwrap() { 1 } else { 8 });
            let (offset, _size) = region.offset_size();
            if let Some(region_bytes) = region_buckets.get_mut(&region) {
                let est_offset = region_bytes.len() + offset;
                region_bytes.extend(string.into_bytes(Some(est_offset)));
            }
        }
    }

    let write_elf_binary = OpenOptions::new().write(true).open(dest).await?;
    let mut bw = BufWriter::new(write_elf_binary);
    // let read_elf_binary = OpenOptions::new().read(true).open(dest).await?;
    // let mut br = BufReader::new(read_elf_binary);

    for (region, bytes) in region_buckets {
        let (offset, _size) = region.offset_size();
        // br.seek(SeekFrom::Start(offset as u64)).await?;
        // let mut existing_region_bytes = vec![0u8; bytes.len()];
        // br.read_exact(&mut existing_region_bytes).await?;
        // let a = encode_hex(&bytes);
        // let b = encode_hex(&existing_region_bytes);
        // if a != b {
        //     error!("Mismatch in region {region:?}\n     new: {a}\nexisting: {b}");
        // }

        bw.seek(SeekFrom::Start(offset as u64)).await?;
        bw.write_all(&bytes).await?;
    }

    jumplists::patch_strings(
        &mut bw,
        mapnames,
        MAPNAMES_JUMPLIST_START,
        MAPNAMES_JUMPLIST_FIELDS,
    )
    .await?;
    songs::patch(&mut bw, songs).await?;
    items::patch(&mut bw, items).await?;
    jumplists::patch_strings(
        &mut bw,
        misc_strings,
        MISC_STRINGS_JUMPLIST_START,
        MISC_STRINGS_JUMPLIST_FIELDS,
    )
    .await?;
    memcard_opts::patch(&mut bw, memcard_opts).await?;
    techniques::patch(&mut bw, techniques).await?;
    enemies::patch(&mut bw, enemies).await?;
    jumplists::patch_strings(
        &mut bw,
        menu_text,
        MENU_TEXT_JUMPLIST_START,
        MENU_TEXT_JUMPLIST_FIELDS,
    )
    .await?;
    jumplists::patch_strings(
        &mut bw,
        item_descriptions,
        ITEM_DESCRIPTION_JUMPLIST_START,
        ITEM_DESCRIPTION_JUMPLIST_FIELDS,
    )
    .await?;
    jumplists::patch_strings(
        &mut bw,
        savexit,
        SAVEXIT_JUMPLIST_START,
        SAVEXIT_JUMPLIST_FIELDS,
    )
    .await?;
    jumplists::patch_strings(
        &mut bw,
        question,
        QUESTION_JUMPLIST_START,
        QUESTION_JUMPLIST_FIELDS,
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
            &item_a.get_relative_pointer().string_position,
            &item_b.get_relative_pointer().string_position,
        )
    });

    // let mut debug_buckets = BTreeMap::new();
    let mut updated_items = BTreeMap::new();
    // let mut relative_pointers = BTreeMap::new();
    for (num, mut item) in sorted_by_ptr_address {
        item.convert_text();
        item.pad_text(if *crate::ENGRISH.get().unwrap() { 1 } else { 8 });
        let text = item.get_text();
        let relative_pointer = item.get_relative_pointer();
        let region = relative_pointer.string_mem_region;
        let position = relative_pointer.string_position;
        let aliased = relative_pointer.string_aliased;
        // Get the bytes we have for this region bucket for adding more string data, if necessary
        let region_bytes = region_buckets.entry(region).or_default();
        let (location, max_size) = region.offset_size();
        let offset = region_bytes.len() + location;
        let mut new_region_size = region_bytes.len();
        if !aliased {
            new_region_size += text.byte_len();
        }
        if new_region_size <= max_size {
            let adjusted_offset = if aliased {
                // If the pointer is an alias, don't add anything to the region and simply get the aliased pointer address
                interner
                    .get(&(region, usize::try_from(position.0).unwrap()))
                    .copied()
                    .unwrap()
            } else {
                // Add the string to the current region if it's not aliased, and return the new pointer address
                region_bytes.extend(text.clone().into_bytes(Some(offset)));
                let rp = item.get_relative_pointer();
                interner.insert(
                    (
                        rp.string_mem_region,
                        usize::try_from(rp.string_position.0).unwrap(),
                    ),
                    offset,
                );
                offset
            };
            item.set_vma_pointer(u32::try_from(adjusted_offset).unwrap() + POINTER_OFFSET);
            updated_items.insert(num, item);
        } else {
            for (disp_region, bytes) in region_buckets.iter() {
                error!("{disp_region:?}: {}\n", encode_hex(bytes));
            }
            error!(
                "Region {region:?} exceeded length. Expected: {max_size}, Got: {new_region_size}. Exceeded by {} bytes.",
                new_region_size - max_size
            );
        }
    }
    updated_items
}

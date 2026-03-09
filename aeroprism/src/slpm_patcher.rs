#![allow(clippy::arbitrary_source_item_ordering, reason = "not needed")]
#[cfg(target_os = "windows")]
use crate::helpers::unset_readonly;
use crate::{
    events::{
        DialogString,
        codec::{decode_psg2_string, parse_next_event_char},
        load_exec_patch,
    },
    helpers::{
        deserialize_u8_hex, deserialize_u16_hex, deserialize_u32_hex, hex_edit_encode,
        save_binary_file, serialize_u8_hex, serialize_u16_hex, serialize_u32_hex,
    },
};
use core::mem::size_of;
use log::{Level, debug, info, log_enabled, warn};
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

static ENEMY_STRUCTS_START: usize = 0x1A_422C;
static ENEMY_STRUCT_SIZE: usize = 148;
static ENEMY_STRUCT_COUNT: usize = 124;
static ENEMY_STRUCT_FIELDS: usize = ENEMY_STRUCT_SIZE / size_of::<u32>();

static ITEM_STRUCTS_START: usize = 0x18_B1D0;
static ITEM_STRUCT_SIZE: usize = 32;
static ITEM_STRUCT_COUNT: usize = 185;
static ITEM_STRUCT_FIELDS: usize = ITEM_STRUCT_SIZE / size_of::<u32>();

// static MAPNAMES_JUMPLIST_START: usize = 0x14_F798;
// static MAPNAMES_POINTER_COUNT: usize = 106;

static END_CREDITS_START: usize = 0x1A_0ED4;
static END_CREDITS_END: usize = 0x1A_1C6C;
static END_CREDITS_BLOB_SIZE: usize = END_CREDITS_END - END_CREDITS_START;
static CREDIT_ITEM_HEADER_SIZE: usize = size_of::<u32>() * 2;
static CREDIT_FOOTER: [u8; CREDIT_ITEM_HEADER_SIZE] =
    [0x01, 0x00, 0x2c, 0x01, 0x00, 0x00, 0x00, 0x00];

#[derive(Serialize, Deserialize)]
pub struct ExecData {
    pub items: Box<[ItemInfo]>,
    pub enemies: Box<[EnemyInfo]>,
    // pub strings: Vec<String>,
    pub end_credits: Box<[EndCreditItem]>,
}

#[repr(u8)]
#[derive(Serialize, Deserialize, Default, PartialEq, Copy, Clone)]
enum ItemEquipSlot {
    #[default]
    None,
    OneHand = 1,
    TwoHand = 2,
    Head = 3,
    Shield = 4,
    Torso = 5,
    Feet = 6,
}

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

#[inline]
fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}

#[inline]
#[expect(clippy::trivially_copy_pass_by_ref, reason = "ref required for serde")]
const fn is_u16_max(val: &u16) -> bool {
    *val == u16::MAX
}

#[inline]
const fn max_u16() -> u16 {
    0xffff
}

#[derive(Serialize, Deserialize)]
pub struct ItemInfo {
    #[serde(
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex"
    )]
    item_number: u32,
    #[serde(
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex"
    )]
    name_pointer: u32,
    item_name: DialogString,
    #[serde(default, skip_serializing_if = "is_default")]
    equip_slot: ItemEquipSlot,
    #[serde(
        default,
        serialize_with = "serialize_u8_hex",
        deserialize_with = "deserialize_u8_hex",
        skip_serializing_if = "is_default"
    )]
    field_1: u8,
    #[serde(
        default = "max_u16",
        serialize_with = "serialize_u16_hex",
        deserialize_with = "deserialize_u16_hex",
        skip_serializing_if = "is_u16_max"
    )]
    field_2: u16,
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
    #[serde(default, skip_serializing_if = "is_default")]
    can_equip: Box<[Character]>,
    #[serde(
        default,
        serialize_with = "serialize_u8_hex",
        deserialize_with = "deserialize_u8_hex",
        skip_serializing_if = "is_default"
    )]
    field_5: u8,
    #[serde(
        default,
        serialize_with = "serialize_u16_hex",
        deserialize_with = "deserialize_u16_hex",
        skip_serializing_if = "is_default"
    )]
    field_6: u16,
    #[serde(default, skip_serializing_if = "is_default")]
    attack: i16,
    #[serde(default, skip_serializing_if = "is_default")]
    defense: i16,
    #[serde(default, skip_serializing_if = "is_default")]
    skill: i16,
    #[serde(default, skip_serializing_if = "is_default")]
    agility: i16,
    #[serde(default, skip_serializing_if = "is_default")]
    luck: i16,
    #[serde(
        default,
        serialize_with = "serialize_u16_hex",
        deserialize_with = "deserialize_u16_hex",
        skip_serializing_if = "is_default"
    )]
    field_7: u16,
}

#[repr(u8)]
#[derive(EnumIter, Serialize, Deserialize, Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Debug)]
enum Elemental {
    Fire = 0x01,
    Ice = 0x02,
    Air = 0x04,
    Lightning = 0x08,
}

#[repr(u8)]
#[derive(Serialize, Deserialize, Default, Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Debug)]
enum EnemyType {
    #[default]
    Demonic, // First and second bits turned off. Effectively, the below two bits count as a weakness to certain techniques. This simply indicates immunity to both biologic and robitic techniques.
    Biologic = 0x01,
    Robotic = 0x02,
}

#[derive(Serialize, Deserialize, Default, Debug)]
pub struct EnemyAttributes {
    // #[serde(
    //     serialize_with = "serialize_u32_hex",
    //     deserialize_with = "deserialize_u32_hex"
    // )]
    // attribute_field: u32, // All bytes of the attribute field. The below values will overwrite the data in this field if it is changed.
    #[serde(default, skip_serializing_if = "is_default")]
    resistances: Box<[Elemental]>, // First four bits of first byte of attribute field
    #[serde(default, skip_serializing_if = "is_default")]
    weaknesses: Box<[Elemental]>, // Second four bits of first byte of attribute field
    #[serde(default, skip_serializing_if = "is_default")]
    field_1: u8, // Second byte of attribute field. Always appears to be zero.
    r#type: EnemyType, // First four bits of third byte of attribute field
    #[serde(default, skip_serializing_if = "is_default")]
    boss: bool, // Mask: 0x04. The name is just a guess. Possessed by Dark Falz, Motherbrain, Neifirst (both occurrences) and Army Eye. No idea what it does.
    #[serde(default, skip_serializing_if = "is_default")]
    super_boss: bool, // Mask: 0x08. As above, the name is just a guess. Only Dark Falz and Motherbrain appear to have the bit for this set. As above, no idea what it does.
    #[serde(default, skip_serializing_if = "is_default")]
    field_2: u8, // Second four bits of third byte of attribute field. Always appears to be zero.
    #[serde(
        default,
        serialize_with = "serialize_u8_hex",
        deserialize_with = "deserialize_u8_hex",
        skip_serializing_if = "is_default"
    )]
    animation: u8, // Fourth byte of attribute field. Controls graphical effects such as whether the enemy floats, sits still, flashes, and others.
}

impl From<[u8; 4]> for EnemyAttributes {
    #[inline]
    fn from(attr_field: [u8; 4]) -> Self {
        // Un-bitpack the attributes field
        let mut attributes = Self::default();
        // attributes.attribute_field = value;
        let resistances = attr_field[0] >> 4;
        let weaknesses = attr_field[0] & 0xf;
        let mut r = Vec::with_capacity(4);
        let mut w = Vec::with_capacity(4);
        for ele in Elemental::iter() {
            if resistances & ele as u8 == ele as u8 {
                r.push(ele);
            }
            if weaknesses & ele as u8 == ele as u8 {
                w.push(ele);
            }
        }
        attributes.resistances = r.into_boxed_slice();
        attributes.weaknesses = w.into_boxed_slice();
        attributes.field_1 = attr_field[1];
        let enemy_types = attr_field[2] >> 4;
        attributes.field_2 = attr_field[2] & 0xf;
        if enemy_types & 0x1 == 0x1 {
            attributes.r#type = EnemyType::Biologic;
        } else if enemy_types & 0x2 == 0x2 {
            attributes.r#type = EnemyType::Robotic;
        } else if enemy_types & 0x3 == 0x3 {
            warn!("Enemy flagged as both biologic AND robitic! This is invalid.");
        }
        if enemy_types & 0x4 == 0x4 {
            attributes.boss = true;
        }
        if enemy_types & 0x8 == 0x8 {
            attributes.super_boss = true;
        }
        attributes.animation = attr_field[3];
        attributes
    }
}

impl From<&EnemyAttributes> for u32 {
    #[inline]
    fn from(value: &EnemyAttributes) -> Self {
        // Re-bitpack the attributes field
        let EnemyAttributes {
            // attribute_field,
            resistances,
            weaknesses,
            field_1,
            r#type,
            boss,
            super_boss,
            field_2,
            animation,
        } = value;
        // Fill resistances and weaknesses byte
        let mut rw = 0;
        for ele in resistances {
            rw |= (*ele as u8) << 4;
        }
        for ele in weaknesses {
            rw |= *ele as u8;
        }
        let mut etype = *r#type as u8;
        if *boss {
            etype |= 0x4;
        }
        if *super_boss {
            etype |= 0x8;
        }
        etype <<= 4;
        etype |= field_2;
        Self::from_be_bytes([rw, *field_1, etype, *animation])
    }
}

#[derive(Serialize, Deserialize)]
pub struct EnemyInfo {
    enemy_number: usize,
    enemy_name: String,
    #[serde(
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex"
    )]
    name_pointer: u32, // Pointer (alias?) to the enemy name string. First field.
    #[serde(flatten)]
    attributes: EnemyAttributes,
    health: u32, // Third field
    attack: u32, // Fourth field
    #[serde(default, skip_serializing_if = "is_default")]
    defense: u32, // Fifth field
    #[serde(default, skip_serializing_if = "is_default")]
    agility: u32, // Sixth field. Controls chance to dodge your hits, possibly others.
    #[serde(
        default,
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex",
        skip_serializing_if = "is_default"
    )]
    field_7: u32, // These fields serve an unknown purpose. Possible values include: intellect, stamina, technique points
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
    // 12 through 17 appear to control the art assets used for this enemy. E.g. dropping the data in these fields from mother brain into neifirst will make neifirst look like mother brain
    #[serde(
        default,
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex",
        skip_serializing_if = "is_default"
    )]
    art_1: u32, // Field 12
    #[serde(
        default,
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex",
        skip_serializing_if = "is_default"
    )]
    art_2: u32,
    #[serde(
        default,
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex",
        skip_serializing_if = "is_default"
    )]
    art_3: u32,
    #[serde(
        default,
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex",
        skip_serializing_if = "is_default"
    )]
    art_4: u32,
    #[serde(
        default,
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex",
        skip_serializing_if = "is_default"
    )]
    art_5: u32,
    #[serde(
        default,
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex",
        skip_serializing_if = "is_default"
    )]
    art_6: u32, // Field 17
    #[serde(
        default,
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex",
        skip_serializing_if = "is_default"
    )]
    field_18: u32,
    #[serde(
        default,
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex",
        skip_serializing_if = "is_default"
    )]
    field_19: u32,
    #[serde(
        default,
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex",
        skip_serializing_if = "is_default"
    )]
    field_20: u32,
    #[serde(
        default,
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex",
        skip_serializing_if = "is_default"
    )]
    field_21: u32,
    #[serde(
        default,
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex",
        skip_serializing_if = "is_default"
    )]
    field_22: u32,
    #[serde(
        default,
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex",
        skip_serializing_if = "is_default"
    )]
    field_23: u32,
    #[serde(
        default,
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex",
        skip_serializing_if = "is_default"
    )]
    field_24: u32,
    #[serde(
        default,
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex",
        skip_serializing_if = "is_default"
    )]
    field_25: u32,
    #[serde(
        default,
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex",
        skip_serializing_if = "is_default"
    )]
    field_26: u32,
    #[serde(
        default,
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex",
        skip_serializing_if = "is_default"
    )]
    field_27: u32,
    #[serde(
        default,
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex",
        skip_serializing_if = "is_default"
    )]
    field_28: u32,
    #[serde(
        default,
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex",
        skip_serializing_if = "is_default"
    )]
    field_29: u32,
    #[serde(
        default,
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex",
        skip_serializing_if = "is_default"
    )]
    field_30: u32,
    #[serde(
        default,
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex",
        skip_serializing_if = "is_default"
    )]
    field_31: u32,
    #[serde(
        default,
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex",
        skip_serializing_if = "is_default"
    )]
    field_32: u32,
    #[serde(
        default,
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex",
        skip_serializing_if = "is_default"
    )]
    field_33: u32,
    #[serde(
        default,
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex",
        skip_serializing_if = "is_default"
    )]
    field_34: u32,
    #[serde(
        default,
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex",
        skip_serializing_if = "is_default"
    )]
    field_35: u32,
    #[serde(
        default,
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex",
        skip_serializing_if = "is_default"
    )]
    field_36: u32,
    #[serde(
        default,
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex",
        skip_serializing_if = "is_default"
    )]
    field_37: u32,
}

#[inline]
pub async fn generate_exec_data<P: AsRef<Path> + Send + Sync>(
    out_dir: &P,
    path: &PathBuf,
) -> Result<(), io::Error> {
    let elf_file = fs::File::open(path).await?;
    let mut elf_reader = BufReader::new(elf_file);
    let exec_data = ExecData {
        items: parse_items(&mut elf_reader).await?.into_boxed_slice(),
        enemies: parse_enemies(&mut elf_reader).await?.into_boxed_slice(),
        // strings: parse_map_strings(&mut elf_reader).await?,
        end_credits: parse_end_credits(&mut elf_reader).await?.into_boxed_slice(),
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
    patch_item_info(&mut bw, items).await?;
    patch_end_credits(&mut bw, end_credits).await?;
    patch_enemy_info(&mut bw, enemies).await?;
    bw.flush().await?;
    Ok(())
}

#[inline]
pub async fn patch_enemy_info(
    exec_writer: &mut BufWriter<fs::File>,
    enemies: Box<[EnemyInfo]>,
) -> Result<(), io::Error> {
    exec_writer
        .seek(SeekFrom::Start(ENEMY_STRUCTS_START.try_into().unwrap()))
        .await
        .unwrap();
    assert_eq!(
        ENEMY_STRUCT_COUNT,
        enemies.len(),
        "Enemy count MUST be exact!"
    );
    for enemy_info in enemies {
        exec_writer
            .write_all(&enemy_info.name_pointer.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&u32::from(&enemy_info.attributes).to_be_bytes())
            .await?;
        exec_writer
            .write_all(&enemy_info.health.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&enemy_info.attack.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&enemy_info.defense.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&enemy_info.agility.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&enemy_info.field_7.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&enemy_info.field_8.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&enemy_info.field_9.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&enemy_info.field_10.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&enemy_info.field_11.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&enemy_info.art_1.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&enemy_info.art_2.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&enemy_info.art_3.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&enemy_info.art_4.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&enemy_info.art_5.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&enemy_info.art_6.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&enemy_info.field_18.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&enemy_info.field_19.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&enemy_info.field_20.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&enemy_info.field_21.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&enemy_info.field_22.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&enemy_info.field_23.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&enemy_info.field_24.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&enemy_info.field_25.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&enemy_info.field_26.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&enemy_info.field_27.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&enemy_info.field_28.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&enemy_info.field_29.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&enemy_info.field_30.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&enemy_info.field_31.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&enemy_info.field_32.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&enemy_info.field_33.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&enemy_info.field_34.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&enemy_info.field_35.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&enemy_info.field_36.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&enemy_info.field_37.to_le_bytes())
            .await?;
    }
    Ok(())
}

#[inline]
pub async fn parse_enemies<R: AsyncBufRead + AsyncSeek + Unpin>(
    reader: &mut R,
) -> Result<Vec<EnemyInfo>, io::Error> {
    reader
        .seek(SeekFrom::Start(ENEMY_STRUCTS_START as u64))
        .await
        .unwrap();
    let mut field_bytes = [0u8; 4];
    let mut field_vec = Vec::with_capacity(ENEMY_STRUCT_FIELDS);
    let mut enemies = Vec::with_capacity(ENEMY_STRUCT_COUNT);
    for enemy_no in 0..ENEMY_STRUCT_COUNT {
        for _field_no in 0..ENEMY_STRUCT_FIELDS {
            reader.read_exact(&mut field_bytes).await?;
            field_vec.push(field_bytes);
        }
        field_vec.reverse();
        let enemy = EnemyInfo {
            enemy_number: enemy_no + 1usize,
            enemy_name: String::new(),
            name_pointer: u32::from_le_bytes(field_vec.pop().unwrap()),
            attributes: EnemyAttributes::from(field_vec.pop().unwrap()),
            health: u32::from_le_bytes(field_vec.pop().unwrap()),
            attack: u32::from_le_bytes(field_vec.pop().unwrap()),
            defense: u32::from_le_bytes(field_vec.pop().unwrap()),
            agility: u32::from_le_bytes(field_vec.pop().unwrap()),
            field_7: u32::from_le_bytes(field_vec.pop().unwrap()),
            field_8: u32::from_le_bytes(field_vec.pop().unwrap()),
            field_9: u32::from_le_bytes(field_vec.pop().unwrap()),
            field_10: u32::from_le_bytes(field_vec.pop().unwrap()),
            field_11: u32::from_le_bytes(field_vec.pop().unwrap()),
            art_1: u32::from_le_bytes(field_vec.pop().unwrap()),
            art_2: u32::from_le_bytes(field_vec.pop().unwrap()),
            art_3: u32::from_le_bytes(field_vec.pop().unwrap()),
            art_4: u32::from_le_bytes(field_vec.pop().unwrap()),
            art_5: u32::from_le_bytes(field_vec.pop().unwrap()),
            art_6: u32::from_le_bytes(field_vec.pop().unwrap()),
            field_18: u32::from_le_bytes(field_vec.pop().unwrap()),
            field_19: u32::from_le_bytes(field_vec.pop().unwrap()),
            field_20: u32::from_le_bytes(field_vec.pop().unwrap()),
            field_21: u32::from_le_bytes(field_vec.pop().unwrap()),
            field_22: u32::from_le_bytes(field_vec.pop().unwrap()),
            field_23: u32::from_le_bytes(field_vec.pop().unwrap()),
            field_24: u32::from_le_bytes(field_vec.pop().unwrap()),
            field_25: u32::from_le_bytes(field_vec.pop().unwrap()),
            field_26: u32::from_le_bytes(field_vec.pop().unwrap()),
            field_27: u32::from_le_bytes(field_vec.pop().unwrap()),
            field_28: u32::from_le_bytes(field_vec.pop().unwrap()),
            field_29: u32::from_le_bytes(field_vec.pop().unwrap()),
            field_30: u32::from_le_bytes(field_vec.pop().unwrap()),
            field_31: u32::from_le_bytes(field_vec.pop().unwrap()),
            field_32: u32::from_le_bytes(field_vec.pop().unwrap()),
            field_33: u32::from_le_bytes(field_vec.pop().unwrap()),
            field_34: u32::from_le_bytes(field_vec.pop().unwrap()),
            field_35: u32::from_le_bytes(field_vec.pop().unwrap()),
            field_36: u32::from_le_bytes(field_vec.pop().unwrap()),
            field_37: u32::from_le_bytes(field_vec.pop().unwrap()),
        };
        enemies.push(enemy);
    }
    // Fill in the enemy names
    for enemy in &mut enemies {
        reader
            .seek(SeekFrom::Start(
                u64::from(enemy.name_pointer) - POINTER_OFFSET as u64,
            ))
            .await
            .unwrap();
        let mut string_bytes = Vec::with_capacity(20);
        reader.read_until(0, &mut string_bytes).await?;
        let mut string_bytes_iter = string_bytes.into_iter().peekable();
        let mut engrish_str = Vec::with_capacity(20);
        while let Some(byte) = string_bytes_iter.next()
            && byte != 0
        {
            parse_next_event_char(&mut string_bytes_iter, &mut engrish_str, byte);
        }
        enemy.enemy_name = engrish_str.concat();
    }
    enemies.shrink_to_fit();
    Ok(enemies)
}

#[inline]
pub async fn patch_item_info(
    exec_writer: &mut BufWriter<fs::File>,
    items: Box<[ItemInfo]>,
) -> Result<(), io::Error> {
    exec_writer
        .seek(SeekFrom::Start(ITEM_STRUCTS_START.try_into().unwrap()))
        .await
        .unwrap();
    assert_eq!(ITEM_STRUCT_COUNT, items.len(), "Item count MUST be exact!");
    for item in items {
        let mut equip_byte = 0;
        for character in item.can_equip {
            equip_byte |= character as u8;
        }
        exec_writer
            .write_all(&item.name_pointer.to_le_bytes())
            .await?;
        exec_writer.write_u8(item.equip_slot as u8).await?;
        exec_writer.write_u8(item.field_1).await?;
        exec_writer.write_all(&item.field_2.to_le_bytes()).await?;
        exec_writer.write_all(&item.field_3.to_le_bytes()).await?;
        exec_writer.write_all(&item.field_4.to_le_bytes()).await?;
        exec_writer.write_u8(equip_byte).await?;
        exec_writer.write_u8(item.field_5).await?;
        exec_writer.write_all(&item.field_6.to_le_bytes()).await?;
        exec_writer.write_all(&item.attack.to_le_bytes()).await?;
        exec_writer.write_all(&item.defense.to_le_bytes()).await?;
        exec_writer.write_all(&item.skill.to_le_bytes()).await?;
        exec_writer.write_all(&item.agility.to_le_bytes()).await?;
        exec_writer.write_all(&item.luck.to_le_bytes()).await?;
        exec_writer.write_all(&item.field_7.to_le_bytes()).await?;
    }
    Ok(())
}

#[inline]
pub async fn parse_items<R: AsyncBufRead + AsyncSeek + Unpin>(
    reader: &mut R,
) -> Result<Vec<ItemInfo>, io::Error> {
    reader
        .seek(SeekFrom::Start(ITEM_STRUCTS_START as u64))
        .await
        .unwrap();
    let mut field_bytes = [0u8; 4];
    let mut field_vec = Vec::with_capacity(ITEM_STRUCT_FIELDS);
    let mut items = Vec::with_capacity(ITEM_STRUCT_COUNT);
    for item_no in 0..ITEM_STRUCT_COUNT {
        for _field_no in 0..ITEM_STRUCT_FIELDS {
            reader.read_exact(&mut field_bytes).await?;
            field_vec.push(field_bytes);
        }
        field_vec.reverse();
        let name_pointer = u32::from_le_bytes(field_vec.pop().unwrap());
        let slot_data = field_vec.pop().unwrap();
        let slot_byte = slot_data[0];
        let field_1 = slot_data[1];
        let field_2 = u16::from_le_bytes([slot_data[2], slot_data[3]]);
        let field_3 = u32::from_le_bytes(field_vec.pop().unwrap());
        let field_4 = u32::from_le_bytes(field_vec.pop().unwrap());
        let eq_f3 = field_vec.pop().unwrap();
        let at_de = field_vec.pop().unwrap();
        let sk_ag = field_vec.pop().unwrap();
        let lu_f4 = field_vec.pop().unwrap();
        let character_byte = eq_f3[0];
        let field_5 = eq_f3[1];
        let field_6 = u16::from_le_bytes([eq_f3[2], eq_f3[3]]);
        let attack = i16::from_le_bytes([at_de[0], at_de[1]]);
        let defense = i16::from_le_bytes([at_de[2], at_de[3]]);
        let skill = i16::from_le_bytes([sk_ag[0], sk_ag[1]]);
        let agility = i16::from_le_bytes([sk_ag[2], sk_ag[3]]);
        let luck = i16::from_le_bytes([lu_f4[0], lu_f4[1]]);
        let field_7 = u16::from_le_bytes([lu_f4[2], lu_f4[3]]);

        let equip_slot = match slot_byte {
            1 => ItemEquipSlot::OneHand,
            2 => ItemEquipSlot::TwoHand,
            3 => ItemEquipSlot::Head,
            4 => ItemEquipSlot::Shield,
            5 => ItemEquipSlot::Torso,
            6 => ItemEquipSlot::Feet,
            _ => ItemEquipSlot::None,
        };

        let mut chars = Vec::with_capacity(8);
        for character in Character::iter() {
            if character_byte & (character as u8) == (character as u8) {
                chars.push(character);
            }
        }

        let item = ItemInfo {
            item_number: u32::try_from(item_no + 1).unwrap(),
            item_name: DialogString::default(),
            name_pointer,
            equip_slot,
            field_1,
            field_2,
            field_3,
            field_4,
            field_5,
            field_6,
            can_equip: chars.into_boxed_slice(),
            attack,
            defense,
            skill,
            agility,
            luck,
            field_7,
        };
        items.push(item);
    }
    for item in &mut items {
        reader
            .seek(SeekFrom::Start(
                u64::from(item.name_pointer) - POINTER_OFFSET as u64,
            ))
            .await
            .unwrap();
        let mut string_bytes = Vec::with_capacity(20);
        reader.read_until(0, &mut string_bytes).await?;
        let engrish_str = decode_psg2_string(string_bytes);
        item.item_name = engrish_str;
    }
    items.shrink_to_fit();
    Ok(items)
}

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

#[derive(Serialize, Deserialize)]
pub struct EndCreditItem {
    vertical_space: u16,
    credit_string: DialogString,
}

#[inline]
pub async fn parse_end_credits<R: AsyncBufRead + AsyncSeek + Unpin>(
    reader: &mut R,
) -> Result<Vec<EndCreditItem>, io::Error> {
    reader
        .seek(SeekFrom::Start(END_CREDITS_START as u64))
        .await?;

    let mut credit_items = Vec::with_capacity(255);
    let mut field = [0x0; 2];
    let mut i = 0;
    debug!("Parsing end credits...");
    loop {
        i += 1;
        // The way each "credit header" appears to work is:
        // 01000XXXX 0200YYYY
        // - XXXX is a 16-bit number to indicate how far we should scroll before displaying the string that follows.
        // - YYYY is a 16-bit number to indicate the length in bytes PLUS the first null terminator of the string to.
        // display
        // The string PLUS null terminator that follows then must be padded to the next 32-bit boundary.

        // Move the cursor past the 0x0100 marker
        reader.read_exact(&mut field).await?;
        // Read the vertical space number
        reader.read_exact(&mut field).await?;
        let vertical_space = u16::from_le_bytes(field);
        // Read the 0x0200 marker
        reader.read_exact(&mut field).await?;
        let second_marker = u16::from_le_bytes(field);
        if second_marker != 2 {
            // If the 0x0200 marker is 0x0000, that is the signal to display the "THE END" graphic after scrolling the
            // vertical space distance in the final header.
            debug!("Ended on {i}th credit.");
            break;
        }
        // Move the cursor past the length indicator -- we only need to calculate it dynamically upon patching.
        reader.read_exact(&mut field).await?;

        // Read all of the bytes until the first null terminator
        let mut string_bytes = Vec::with_capacity(32);
        while let Ok(byte) = reader.read_u8().await
            && byte != 0
        {
            string_bytes.push(byte);
        }
        // Move the cursor to the start of the next field

        while reader.stream_position().await? % 4 != 0 {
            reader.read_u8().await?;
        }
        if log_enabled!(Level::Debug) {
            debug!(
                "Raw credit header: {}",
                hex_edit_encode(
                    &[
                        1u16.to_le_bytes(),
                        vertical_space.to_le_bytes(),
                        2u16.to_le_bytes(),
                        (u16::try_from(string_bytes.len()).unwrap() + 1).to_le_bytes()
                    ]
                    .concat()
                )
            );
            debug!("Raw credit string: {}", hex_edit_encode(&string_bytes));
        }
        let engrish_str = decode_psg2_string(string_bytes);
        if log_enabled!(Level::Debug) {
            debug!(
                "Rendered credit string: {}",
                hex_edit_encode(&engrish_str.clone().into_bytes(None))
            );
            debug!("Debugged credit string: {engrish_str:#?}",);
        }
        // Read the next two fields
        credit_items.push(EndCreditItem {
            vertical_space,
            credit_string: engrish_str,
        });
    }
    // reader.read_exact(&mut credit_bytes).await?;
    // let mut credits_iter
    credit_items.shrink_to_fit();
    Ok(credit_items)
}

#[inline]
pub async fn patch_end_credits(
    exec_writer: &mut BufWriter<fs::File>,
    end_credits: Box<[EndCreditItem]>,
) -> Result<(), io::Error> {
    exec_writer
        .seek(SeekFrom::Start(END_CREDITS_START.try_into().unwrap()))
        .await?;
    let mut total_bytes = 0;
    for end_credit_item in end_credits {
        let EndCreditItem {
            vertical_space,
            mut credit_string,
        } = end_credit_item;

        if log_enabled!(Level::Debug) {
            debug!("Debugged credit string: {credit_string:#?}");
        }

        // The header wants the string byte length plus the null terminator
        let credit_string_size = u16::try_from(credit_string.byte_len() + 1).unwrap();
        let credit_header = [
            1u16.to_le_bytes(),
            vertical_space.to_le_bytes(),
            2u16.to_le_bytes(),
            credit_string_size.to_le_bytes(),
        ]
        .concat();
        // Mark as padded so we don't have to calculate that manually here
        credit_string.set_padded();

        // Convert the string into bytes and calculate the length field, storing as a u16 for later
        let expand_by = credit_string.byte_len() + credit_header.len();
        if expand_by + total_bytes + CREDIT_ITEM_HEADER_SIZE > END_CREDITS_BLOB_SIZE {
            if log_enabled!(Level::Warn) {
                warn!(
                    "End credit overflow! Data corruption likely! Overflowed by {} bytes. Stopping at text '{}'",
                    (expand_by + total_bytes + CREDIT_ITEM_HEADER_SIZE)
                        .saturating_sub(END_CREDITS_BLOB_SIZE),
                    credit_string
                );
            }
            break;
        }
        let string_bytes = credit_string.into_bytes(None);

        if log_enabled!(Level::Debug) {
            debug!("Rendered credit string: {}", hex_edit_encode(&string_bytes));
        }

        total_bytes += expand_by;
        exec_writer
            .write_all(&[credit_header, string_bytes].concat())
            .await?;
    }
    // Finalize credits
    exec_writer.write_all(&CREDIT_FOOTER).await?;

    Ok(())
}

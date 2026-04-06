#![allow(clippy::arbitrary_source_item_ordering, reason = "not needed")]
use crate::{
    events::{
        DialogItem, codec::decode_psg2_string, deserialize_dialog_items, serialize_dialog_items,
    },
    helpers::{deserialize_u32_hex, is_default, serialize_u32_hex},
    slpm_patcher::{EnemyType, Hexu32, POINTER_OFFSET, SpellElemental, StringMemRegion},
};
use alloc::collections::BTreeMap;
use core::mem::size_of;
use indexmap::IndexSet;
use serde::{Deserialize, Serialize};
use std::io;
use strum::{EnumIter, IntoEnumIterator};
use tokio::{
    fs::{self},
    io::{AsyncBufRead, AsyncReadExt, AsyncSeek, AsyncSeekExt, AsyncWriteExt, BufWriter, SeekFrom},
};

static ENEMY_STRUCTS_START: usize = 0x1A_422C;
static ENEMY_STRUCT_SIZE: usize = 148;
static ENEMY_STRUCT_COUNT: usize = 124;
static ENEMY_STRUCT_FIELDS: usize = ENEMY_STRUCT_SIZE / size_of::<u32>();

#[derive(Serialize, Deserialize, Default, Debug)]
pub struct EnemyAttributes {
    // #[serde(
    //     serialize_with = "serialize_u32_hex",
    //     deserialize_with = "deserialize_u32_hex"
    // )]
    // attribute_field: u32, // All bytes of the attribute field. The below values will overwrite the data in this field if it is changed.
    #[serde(default, skip_serializing_if = "is_default")]
    resistances: Box<[SpellElemental]>, // First four bits of first byte of attribute field
    #[serde(default, skip_serializing_if = "is_default")]
    weaknesses: Box<[SpellElemental]>, // Second four bits of first byte of attribute field
    #[serde(default, skip_serializing_if = "is_default")]
    field_1: u8, // Second byte of attribute field. Always appears to be zero.
    r#type: Box<[EnemyType]>, // First four bits of third byte of attribute field
    #[serde(default, skip_serializing_if = "is_default")]
    effect: Box<[Effect]>, // Fourth byte of attribute field. First nibble: Flash and two other things, not sure which yet. Second nibble: Controls whether the enemy animation hovers, flies, or neither.
}

#[repr(u8)]
#[derive(EnumIter, Serialize, Deserialize, Debug, Copy, Clone, PartialEq, Eq)]
enum Effect {
    EffectA = 0x10, // I.e. all bosses and most demons
    EffectB = 0x20, // I.e. grass killer, satman (robot)
    EffectC = 0x40, // I.e. eyesore, heavy soldier, some demons
    Fly = 0x01,     // I.e. mosquito and other flying bugs
    Hover = 0x04,   // I.e. spinner and hovering robots
}

impl Effect {
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

impl From<[u8; 4]> for EnemyAttributes {
    // De-bitpack the attributes field
    #[inline]
    fn from(attr_field: [u8; 4]) -> Self {
        Self {
            // attribute_field, value,
            resistances: SpellElemental::from_byte(attr_field[0] >> 4),
            weaknesses: SpellElemental::from_byte(attr_field[0]),
            field_1: attr_field[1],
            r#type: EnemyType::from_byte(attr_field[2]),
            effect: Effect::from_byte(attr_field[3]),
        }
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
            effect: effects,
        } = value;
        // Fill resistances and weaknesses byte
        let mut rw = SpellElemental::to_byte(resistances) << 4;
        rw |= SpellElemental::to_byte(weaknesses);

        Self::from_be_bytes([
            rw,
            *field_1,
            EnemyType::to_byte(r#type),
            Effect::to_byte(effects),
        ])
    }
}

#[derive(Serialize, Deserialize)]
pub struct EnemyInfo {
    #[serde(
        deserialize_with = "deserialize_dialog_items",
        serialize_with = "serialize_dialog_items"
    )]
    name: Vec<DialogItem>,
    #[serde(
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex"
    )]
    name_pointer: u32, // Pointer (alias?) to the enemy name string. First field.
    #[serde(
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex"
    )]
    name_vma_pointer: u32, // Literal VMA pointer to the enemy name string
    relative_name_pointer: (StringMemRegion, Hexu32),
    #[serde(flatten)]
    attributes: EnemyAttributes,
    health: u32, // Third field
    attack: u32, // Fourth field
    #[serde(default, skip_serializing_if = "is_default")]
    defense: u32, // Fifth field
    #[serde(default, skip_serializing_if = "is_default")]
    agility: u32, // Sixth field. Controls chance to dodge your hits, possibly others.
    #[serde(default, skip_serializing_if = "is_default")]
    field_7: i32, // These fields serve an unknown purpose. Possible values include: intellect, stamina, technique points
    #[serde(default, skip_serializing_if = "is_default")]
    field_8: i32,
    #[serde(default, skip_serializing_if = "is_default")]
    field_9: i32,
    #[serde(default, skip_serializing_if = "is_default")]
    field_10: i32,
    #[serde(default, skip_serializing_if = "is_default")]
    field_11: i32,
    // 12 through 17 appear to control the art assets used for this enemy. E.g. dropping the data in these fields from mother brain into neifirst will make neifirst look like mother brain
    #[serde(default)]
    mondat_def: u32, // I think this mondat file describes image layouts
    #[serde(default, skip_serializing_if = "is_default")]
    mondat_body_sprites: u32, // Body art
    #[serde(default, skip_serializing_if = "is_default")]
    mondat_attack_sprites: u32, // Attack art
    #[serde(default, skip_serializing_if = "is_default")]
    mondat_special_attack_sprites: u32, // Special attack art
    #[serde(default, skip_serializing_if = "is_default")]
    mondat_ultimate_attack_sprites: u32, // Ultimate attack art
    #[serde(default, skip_serializing_if = "is_default")]
    mondat_large_body_sprites: u32, // Extra body assets for enemies with large bodies
    #[serde(default, skip_serializing_if = "is_default")]
    field_18: i32,
    #[serde(default, skip_serializing_if = "is_default")]
    field_19: i32,
    #[serde(default, skip_serializing_if = "is_default")]
    field_20: i32,
    #[serde(default, skip_serializing_if = "is_default")]
    field_21: i32,
    #[serde(default, skip_serializing_if = "is_default")]
    field_22: i32,
    #[serde(default, skip_serializing_if = "is_default")]
    field_23: i32,
    #[serde(
        default,
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex",
        skip_serializing_if = "is_default"
    )]
    field_24: u32,
    #[serde(default, skip_serializing_if = "is_default")]
    field_25: i32,
    #[serde(default, skip_serializing_if = "is_default")]
    field_26: i32,
    #[serde(default, skip_serializing_if = "is_default")]
    field_27: i32,
    #[serde(
        default,
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex",
        skip_serializing_if = "is_default"
    )]
    field_28: u32,
    #[serde(default, skip_serializing_if = "is_default")]
    field_29: i32,
    #[serde(default, skip_serializing_if = "is_default")]
    field_30: i32,
    #[serde(default, skip_serializing_if = "is_default")]
    field_31: i32,
    #[serde(
        default,
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex",
        skip_serializing_if = "is_default"
    )]
    field_32: u32,
    #[serde(default, skip_serializing_if = "is_default")]
    field_33: i32,
    #[serde(default, skip_serializing_if = "is_default")]
    field_34: i32,
    #[serde(default, skip_serializing_if = "is_default")]
    field_35: i32,
    #[serde(
        default,
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex",
        skip_serializing_if = "is_default"
    )]
    field_36: u32,
    #[serde(default, skip_serializing_if = "is_default")]
    field_37: i32,
}

#[inline]
pub async fn parse<R: AsyncBufRead + AsyncSeek + Unpin>(
    reader: &mut R,
) -> Result<BTreeMap<Hexu32, EnemyInfo>, io::Error> {
    reader
        .seek(SeekFrom::Start(ENEMY_STRUCTS_START as u64))
        .await
        .unwrap();
    let mut field_bytes = [0u8; 4];
    let mut field_vec = Vec::with_capacity(ENEMY_STRUCT_FIELDS);
    let mut enemies = BTreeMap::new();
    let mut relative_pointer_index = IndexSet::with_capacity(ENEMY_STRUCT_COUNT);
    for enemy_no in 0..ENEMY_STRUCT_COUNT {
        for _field_no in 0..ENEMY_STRUCT_FIELDS {
            reader.read_exact(&mut field_bytes).await?;
            field_vec.push(field_bytes);
        }
        field_vec.reverse();
        let pointer_bytes = field_vec.pop().unwrap();
        let name_pointer =
            u32::from_le_bytes(pointer_bytes) - u32::try_from(POINTER_OFFSET).unwrap();
        relative_pointer_index.insert(name_pointer);

        let enemy = EnemyInfo {
            name: Vec::new(),
            name_pointer,
            name_vma_pointer: u32::from_be_bytes(pointer_bytes),
            relative_name_pointer: (StringMemRegion::RegionA, Hexu32(0)),
            attributes: EnemyAttributes::from(field_vec.pop().unwrap()),
            health: u32::from_le_bytes(field_vec.pop().unwrap()),
            attack: u32::from_le_bytes(field_vec.pop().unwrap()),
            defense: u32::from_le_bytes(field_vec.pop().unwrap()),
            agility: u32::from_le_bytes(field_vec.pop().unwrap()),
            field_7: i32::from_le_bytes(field_vec.pop().unwrap()),
            field_8: i32::from_le_bytes(field_vec.pop().unwrap()),
            field_9: i32::from_le_bytes(field_vec.pop().unwrap()),
            field_10: i32::from_le_bytes(field_vec.pop().unwrap()),
            field_11: i32::from_le_bytes(field_vec.pop().unwrap()),
            mondat_def: u32::from_le_bytes(field_vec.pop().unwrap()),
            mondat_body_sprites: u32::from_le_bytes(field_vec.pop().unwrap()),
            mondat_attack_sprites: u32::from_le_bytes(field_vec.pop().unwrap()),
            mondat_special_attack_sprites: u32::from_le_bytes(field_vec.pop().unwrap()),
            mondat_ultimate_attack_sprites: u32::from_le_bytes(field_vec.pop().unwrap()),
            mondat_large_body_sprites: u32::from_le_bytes(field_vec.pop().unwrap()),
            field_18: i32::from_le_bytes(field_vec.pop().unwrap()),
            field_19: i32::from_le_bytes(field_vec.pop().unwrap()),
            field_20: i32::from_le_bytes(field_vec.pop().unwrap()),
            field_21: i32::from_le_bytes(field_vec.pop().unwrap()),
            field_22: i32::from_le_bytes(field_vec.pop().unwrap()),
            field_23: i32::from_le_bytes(field_vec.pop().unwrap()),
            field_24: u32::from_le_bytes(field_vec.pop().unwrap()),
            field_25: i32::from_le_bytes(field_vec.pop().unwrap()),
            field_26: i32::from_le_bytes(field_vec.pop().unwrap()),
            field_27: i32::from_le_bytes(field_vec.pop().unwrap()),
            field_28: u32::from_le_bytes(field_vec.pop().unwrap()),
            field_29: i32::from_le_bytes(field_vec.pop().unwrap()),
            field_30: i32::from_le_bytes(field_vec.pop().unwrap()),
            field_31: i32::from_le_bytes(field_vec.pop().unwrap()),
            field_32: u32::from_le_bytes(field_vec.pop().unwrap()),
            field_33: i32::from_le_bytes(field_vec.pop().unwrap()),
            field_34: i32::from_le_bytes(field_vec.pop().unwrap()),
            field_35: i32::from_le_bytes(field_vec.pop().unwrap()),
            field_36: u32::from_le_bytes(field_vec.pop().unwrap()),
            field_37: i32::from_le_bytes(field_vec.pop().unwrap()),
        };
        enemies.insert(Hexu32(u32::try_from(enemy_no + 1usize).unwrap()), enemy);
    }
    relative_pointer_index.sort_unstable();
    // use crate::slpm_patcher::Hexu32;
    // use alloc::collections::BTreeMap;
    // use crate::helpers::{save_binary_file, encode_hex};
    // use std::path::PathBuf;
    // let mut enemy_pointers = BTreeMap::new();
    // Fill in the enemy names and relative pointers
    for enemy in enemies.values_mut() {
        reader
            .seek(SeekFrom::Start(u64::from(enemy.name_pointer)))
            .await
            .unwrap();
        let mut string_bytes = Vec::with_capacity(20);
        while let Ok(byte) = reader.read_u8().await
            && byte != 0
        {
            string_bytes.push(byte);
        }
        enemy.name = decode_psg2_string(string_bytes).text;
        let region = StringMemRegion::try_from(enemy.name_pointer).unwrap();
        let index = relative_pointer_index
            .get_index_of(&enemy.name_pointer)
            .unwrap();
        enemy.relative_name_pointer = (region, Hexu32(u32::try_from(index).unwrap()));
        // enemy_pointers.insert(
        //     Hexu32(enemy.name_pointer - 0xff000),
        //     (
        //         encode_hex(&enemy.name_pointer.to_le_bytes()),
        //         enemy.enemy_name.to_string(),
        //     ),
        // );
    }
    // let bytes = serde_json::to_string_pretty(&enemy_pointers)
    //     .unwrap()
    //     .into_bytes();
    // save_binary_file(&PathBuf::from("eng_enemy_pointers.json"), &bytes).await?;
    Ok(enemies)
}

#[inline]
pub async fn patch(
    exec_writer: &mut BufWriter<fs::File>,
    enemies: BTreeMap<Hexu32, EnemyInfo>,
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
    for (_, enemy_info) in enemies {
        exec_writer
            .write_all(&enemy_info.name_vma_pointer.to_be_bytes())
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
            .write_all(&enemy_info.mondat_def.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&enemy_info.mondat_body_sprites.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&enemy_info.mondat_attack_sprites.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&enemy_info.mondat_special_attack_sprites.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&enemy_info.mondat_ultimate_attack_sprites.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&enemy_info.mondat_large_body_sprites.to_le_bytes())
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

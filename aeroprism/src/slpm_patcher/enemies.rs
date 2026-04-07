#![allow(clippy::arbitrary_source_item_ordering, reason = "not needed")]
use crate::{
    events::{
        DialogItem, DialogString, codec::decode_psg2_string, deserialize_dialog_items,
        serialize_dialog_items,
    },
    helpers::{deserialize_u32_hex, is_default, serialize_u32_hex},
    slpm_patcher::{
        EnemyType, Hexu32, POINTER_OFFSET, RelativePointer, SpellElemental, StringMemRegion,
    },
};
use alloc::collections::BTreeMap;
use core::mem::size_of;
use indexmap::IndexSet;
use serde::{Deserialize, Serialize};
use std::io;
use string_interner::symbol::SymbolU32;
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
    unknown_1: u8, // Second byte of attribute field. Always appears to be zero.
    r#type: Box<[EnemyType]>, // First four bits of third byte of attribute field
    #[serde(default, skip_serializing_if = "is_default")]
    graphic_effect: Box<[GraphicEffect]>, // Fourth byte of attribute field. First nibble: Whether and how large the enemy casts a shadow. Second nibble: Controls whether the enemy animation hovers, flies, or neither.
}

#[repr(u8)]
#[derive(EnumIter, Serialize, Deserialize, Debug, Copy, Clone, PartialEq, Eq, Default)]
enum GraphicEffect {
    #[default]
    #[serde(alias = "smallshadow", alias = "small_shadow")]
    SmallShadow = 0x00,
    #[serde(alias = "noshadow", alias = "no_shadow")]
    NoShadow = 0x10, // I.e. all bosses and most demons
    #[serde(alias = "mediumshadow", alias = "medium_shadow")]
    MediumShadow = 0x20, // I.e. grass killer, satman (robot)
    #[serde(alias = "largeshadow", alias = "large_shadow")]
    LargeShadow = 0x40, // I.e. eyesore, heavy soldier, some demons
    #[serde(alias = "fly")]
    Fly = 0x01, // I.e. mosquito and other flying bugs
    #[serde(alias = "hover")]
    Hover = 0x04, // I.e. spinner and hovering robots
}

impl GraphicEffect {
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
        if byte & 0xf0 == 0 {
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

impl From<[u8; 4]> for EnemyAttributes {
    // De-bitpack the attributes field
    #[inline]
    fn from(attr_field: [u8; 4]) -> Self {
        Self {
            // attribute_field, value,
            resistances: SpellElemental::from_byte(attr_field[0] >> 4),
            weaknesses: SpellElemental::from_byte(attr_field[0]),
            unknown_1: attr_field[1],
            r#type: EnemyType::from_byte(attr_field[2]),
            graphic_effect: GraphicEffect::from_byte(attr_field[3]),
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
            unknown_1,
            r#type,
            graphic_effect: effects,
        } = value;
        // Fill resistances and weaknesses byte
        let mut rw = SpellElemental::to_byte(resistances) << 4;
        rw |= SpellElemental::to_byte(weaknesses);

        Self::from_be_bytes([
            rw,
            *unknown_1,
            EnemyType::to_byte(r#type),
            GraphicEffect::to_byte(effects),
        ])
    }
}

#[derive(Serialize, Deserialize)]
pub struct EnemyInfo {
    #[serde(
        deserialize_with = "deserialize_dialog_items",
        serialize_with = "serialize_dialog_items"
    )]
    pub name: Vec<DialogItem>,
    #[serde(
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex"
    )]
    pub name_pointer: u32, // Pointer (alias?) to the enemy name string. First field.
    #[serde(
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex"
    )]
    pub name_vma_pointer: u32, // Literal VMA pointer to the enemy name string
    #[serde(skip)]
    pub interner: Option<SymbolU32>,
    #[serde(skip)]
    pub text: DialogString,
    pub relative_name_pointer: (StringMemRegion, Hexu32),
    #[serde(flatten)]
    pub attributes: EnemyAttributes,
    pub health: u32, // Third field
    pub attack: u32, // Fourth field
    #[serde(default, skip_serializing_if = "is_default")]
    pub defense: u32, // Fifth field
    #[serde(default, skip_serializing_if = "is_default")]
    pub agility: u32, // Sixth field. Controls chance to dodge your hits, possibly others.
    #[serde(default, skip_serializing_if = "is_default")]
    pub unknown_7: i32, // These fields serve an unknown purpose. Possible values include: intellect, stamina, technique points
    #[serde(default, skip_serializing_if = "is_default")]
    pub unknown_8: i32,
    #[serde(default, skip_serializing_if = "is_default")]
    pub unknown_9: i32,
    #[serde(default, skip_serializing_if = "is_default")]
    pub unknown_10: i32,
    #[serde(default, skip_serializing_if = "is_default")]
    pub unknown_11: i32,
    // 12 through 17 appear to control the art assets used for this enemy. E.g. dropping the data in these fields from mother brain into neifirst will make neifirst look like mother brain
    #[serde(default)]
    pub mondat_def: u32, // I think this mondat file describes image layouts
    #[serde(default, skip_serializing_if = "is_default")]
    pub mondat_body_sprites: u32, // Body art
    #[serde(default, skip_serializing_if = "is_default")]
    pub mondat_attack_sprites: u32, // Attack art
    #[serde(default, skip_serializing_if = "is_default")]
    pub mondat_special_attack_sprites: u32, // Special attack art
    #[serde(default, skip_serializing_if = "is_default")]
    pub mondat_ultimate_attack_sprites: u32, // Ultimate attack art
    #[serde(default, skip_serializing_if = "is_default")]
    pub mondat_large_body_sprites: u32, // Extra body assets for enemies with large bodies
    #[serde(default, skip_serializing_if = "is_default")]
    pub unknown_18: i32,
    #[serde(default, skip_serializing_if = "is_default")]
    pub unknown_19: i32,
    #[serde(default, skip_serializing_if = "is_default")]
    pub unknown_20: i32,
    #[serde(default, skip_serializing_if = "is_default")]
    pub unknown_21: i32,
    #[serde(default, skip_serializing_if = "is_default")]
    pub unknown_22: i32,
    #[serde(default, skip_serializing_if = "is_default")]
    pub unknown_23: i32,
    #[serde(
        default,
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex",
        skip_serializing_if = "is_default"
    )]
    pub unknown_24: u32,
    #[serde(default, skip_serializing_if = "is_default")]
    pub unknown_25: i32,
    #[serde(default, skip_serializing_if = "is_default")]
    pub unknown_26: i32,
    #[serde(default, skip_serializing_if = "is_default")]
    pub unknown_27: i32,
    #[serde(
        default,
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex",
        skip_serializing_if = "is_default"
    )]
    pub unknown_28: u32,
    #[serde(default, skip_serializing_if = "is_default")]
    pub unknown_29: i32,
    #[serde(default, skip_serializing_if = "is_default")]
    pub unknown_30: i32,
    #[serde(default, skip_serializing_if = "is_default")]
    pub unknown_31: i32,
    #[serde(
        default,
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex",
        skip_serializing_if = "is_default"
    )]
    pub unknown_32: u32,
    #[serde(default, skip_serializing_if = "is_default")]
    pub unknown_33: i32,
    #[serde(default, skip_serializing_if = "is_default")]
    pub unknown_34: i32,
    #[serde(default, skip_serializing_if = "is_default")]
    pub unknown_35: i32,
    #[serde(
        default,
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex",
        skip_serializing_if = "is_default"
    )]
    pub unknown_36: u32,
    #[serde(default, skip_serializing_if = "is_default")]
    pub unknown_37: i32,
}

impl RelativePointer for EnemyInfo {
    fn get_symbol_mut(&mut self) -> &mut Option<SymbolU32> {
        &mut self.interner
    }

    fn get_relative_pointer(&self) -> (StringMemRegion, Hexu32) {
        (self.relative_name_pointer.0, self.relative_name_pointer.1)
    }

    fn get_text(&'_ self) -> &'_ DialogString {
        &self.text
    }

    fn pad_text(&mut self, size: u8) {
        self.text.set_padding(size);
    }

    fn set_vma_pointer(&mut self, ptr_le: u32) {
        self.name_vma_pointer = ptr_le;
    }

    fn convert_text(&mut self) {
        self.text = DialogString {
            text: self.name.clone(),
            padding: 0,
        };
    }
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
            interner: None,
            text: DialogString::default(),
            relative_name_pointer: (StringMemRegion::RegionA, Hexu32(0)),
            attributes: EnemyAttributes::from(field_vec.pop().unwrap()),
            health: u32::from_le_bytes(field_vec.pop().unwrap()),
            attack: u32::from_le_bytes(field_vec.pop().unwrap()),
            defense: u32::from_le_bytes(field_vec.pop().unwrap()),
            agility: u32::from_le_bytes(field_vec.pop().unwrap()),
            unknown_7: i32::from_le_bytes(field_vec.pop().unwrap()),
            unknown_8: i32::from_le_bytes(field_vec.pop().unwrap()),
            unknown_9: i32::from_le_bytes(field_vec.pop().unwrap()),
            unknown_10: i32::from_le_bytes(field_vec.pop().unwrap()),
            unknown_11: i32::from_le_bytes(field_vec.pop().unwrap()),
            mondat_def: u32::from_le_bytes(field_vec.pop().unwrap()),
            mondat_body_sprites: u32::from_le_bytes(field_vec.pop().unwrap()),
            mondat_attack_sprites: u32::from_le_bytes(field_vec.pop().unwrap()),
            mondat_special_attack_sprites: u32::from_le_bytes(field_vec.pop().unwrap()),
            mondat_ultimate_attack_sprites: u32::from_le_bytes(field_vec.pop().unwrap()),
            mondat_large_body_sprites: u32::from_le_bytes(field_vec.pop().unwrap()),
            unknown_18: i32::from_le_bytes(field_vec.pop().unwrap()),
            unknown_19: i32::from_le_bytes(field_vec.pop().unwrap()),
            unknown_20: i32::from_le_bytes(field_vec.pop().unwrap()),
            unknown_21: i32::from_le_bytes(field_vec.pop().unwrap()),
            unknown_22: i32::from_le_bytes(field_vec.pop().unwrap()),
            unknown_23: i32::from_le_bytes(field_vec.pop().unwrap()),
            unknown_24: u32::from_le_bytes(field_vec.pop().unwrap()),
            unknown_25: i32::from_le_bytes(field_vec.pop().unwrap()),
            unknown_26: i32::from_le_bytes(field_vec.pop().unwrap()),
            unknown_27: i32::from_le_bytes(field_vec.pop().unwrap()),
            unknown_28: u32::from_le_bytes(field_vec.pop().unwrap()),
            unknown_29: i32::from_le_bytes(field_vec.pop().unwrap()),
            unknown_30: i32::from_le_bytes(field_vec.pop().unwrap()),
            unknown_31: i32::from_le_bytes(field_vec.pop().unwrap()),
            unknown_32: u32::from_le_bytes(field_vec.pop().unwrap()),
            unknown_33: i32::from_le_bytes(field_vec.pop().unwrap()),
            unknown_34: i32::from_le_bytes(field_vec.pop().unwrap()),
            unknown_35: i32::from_le_bytes(field_vec.pop().unwrap()),
            unknown_36: u32::from_le_bytes(field_vec.pop().unwrap()),
            unknown_37: i32::from_le_bytes(field_vec.pop().unwrap()),
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
            .write_all(&enemy_info.unknown_7.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&enemy_info.unknown_8.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&enemy_info.unknown_9.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&enemy_info.unknown_10.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&enemy_info.unknown_11.to_le_bytes())
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
            .write_all(&enemy_info.unknown_18.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&enemy_info.unknown_19.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&enemy_info.unknown_20.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&enemy_info.unknown_21.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&enemy_info.unknown_22.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&enemy_info.unknown_23.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&enemy_info.unknown_24.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&enemy_info.unknown_25.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&enemy_info.unknown_26.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&enemy_info.unknown_27.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&enemy_info.unknown_28.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&enemy_info.unknown_29.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&enemy_info.unknown_30.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&enemy_info.unknown_31.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&enemy_info.unknown_32.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&enemy_info.unknown_33.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&enemy_info.unknown_34.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&enemy_info.unknown_35.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&enemy_info.unknown_36.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&enemy_info.unknown_37.to_le_bytes())
            .await?;
    }
    Ok(())
}

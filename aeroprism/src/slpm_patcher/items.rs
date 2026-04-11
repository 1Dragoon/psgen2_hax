// use crate::helpers::{deserialize_u32_hex, encode_hex, serialize_u32_hex};
// use log::warn;
use crate::{
    events::{
        DialogItem, DialogString, codec::decode_psg2_string, deserialize_dialog_items,
        serialize_dialog_items,
    },
    helpers::{
        Hexu32, deserialize_u8_hex, deserialize_u16_hex, is_default, is_u16_max, max_u16,
        serialize_u8_hex, serialize_u16_hex,
    },
    slpm_patcher::{POINTER_OFFSET, RelativePointerInfo, StringFill},
};
use alloc::collections::BTreeMap;
use serde::{Deserialize, Serialize};
use std::io;
use strum::{EnumIter, IntoEnumIterator};
use tokio::{
    fs::{self},
    io::{AsyncBufRead, AsyncReadExt, AsyncSeek, AsyncSeekExt, AsyncWriteExt, BufWriter, SeekFrom},
};

static ITEM_STRUCTS_START: usize = 0x18_B1B0;
static ITEM_STRUCT_COUNT: usize = 186;
static ITEM_STRUCT_FIELDS: usize = 8;

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

#[repr(u8)]
#[derive(Serialize, Deserialize, Default, Eq, PartialEq, Copy, Clone)]
pub enum ItemEquipSlot {
    #[default]
    #[serde(alias = "none")]
    None = 0,
    #[serde(alias = "onehand")]
    OneHand = 1,
    #[serde(alias = "twohand")]
    TwoHand = 2,
    #[serde(alias = "head")]
    Head = 3,
    #[serde(alias = "shield")]
    Shield = 4,
    #[serde(alias = "torso")]
    Torso = 5,
    #[serde(alias = "feet")]
    Feet = 6,
}

impl TryFrom<i16> for ItemEquipSlot {
    type Error = String;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::None),
            1 => Ok(Self::OneHand),
            2 => Ok(Self::TwoHand),
            3 => Ok(Self::Head),
            4 => Ok(Self::Shield),
            5 => Ok(Self::Torso),
            6 => Ok(Self::Feet),
            other => Err(format!("Invalid equip slot value {other}")),
        }
    }
}

#[expect(
    clippy::arbitrary_source_item_ordering,
    reason = "Ordered by binary struct fields."
)]
#[derive(Serialize, Deserialize)]
pub struct ItemInfo {
    #[serde(
        deserialize_with = "deserialize_dialog_items",
        serialize_with = "serialize_dialog_items"
    )]
    pub name: Vec<DialogItem>,
    // #[serde(
    //     serialize_with = "serialize_u32_hex",
    //     deserialize_with = "deserialize_u32_hex"
    // )]
    #[serde(skip)]
    pub name_vma_pointer: u32,
    #[serde(skip)]
    pub text: DialogString,
    #[serde(flatten)]
    pub relative_name_pointer: RelativePointerInfo,
    #[serde(default, skip_serializing_if = "is_default")]
    pub equip_slot: ItemEquipSlot,
    #[serde(
        default = "max_u16",
        serialize_with = "serialize_u16_hex",
        deserialize_with = "deserialize_u16_hex",
        skip_serializing_if = "is_u16_max"
    )]
    pub unknown_2: u16, // Always seems to be 0xFFFF, maybe indicates unitialized data to align to next i32?
    #[serde(default, skip_serializing_if = "is_default")]
    pub buy_price: i32,
    #[serde(default, skip_serializing_if = "is_default")]
    pub sell_price: i32,
    #[serde(default, skip_serializing_if = "is_default")]
    pub can_equip: Box<[Character]>, // Character mask
    #[serde(
        default,
        serialize_with = "serialize_u8_hex",
        deserialize_with = "deserialize_u8_hex",
        skip_serializing_if = "is_default"
    )]
    pub unknown_5: u8, // Appears unused, probably MSB of 16-bit character mask above
    #[serde(default, skip_serializing_if = "is_default")]
    pub enchantment: Box<[Enchant]>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub important: bool, // Indicates whether items are allowed to be sold, discarded, etc.
    // #[serde(
    //     default,
    //     serialize_with = "serialize_u32_hex",
    //     deserialize_with = "deserialize_u32_hex",
    //     skip_serializing_if = "is_default"
    // )]
    // attributes: u32,
    #[serde(default, skip_serializing_if = "is_default")]
    pub attack: i16,
    #[serde(default, skip_serializing_if = "is_default")]
    pub defense: i16,
    #[serde(default, skip_serializing_if = "is_default")]
    pub skill: i16,
    #[serde(default, skip_serializing_if = "is_default")]
    pub agility: i16,
    #[serde(default, skip_serializing_if = "is_default")]
    pub luck: i16,
    #[serde(
        default,
        serialize_with = "serialize_u16_hex",
        deserialize_with = "deserialize_u16_hex",
        skip_serializing_if = "is_default"
    )]
    pub unknown_7: u16, // Appears unused, probably final padding of this struct to align on 32-bit boundary
}

impl StringFill for ItemInfo {
    fn convert_text(&mut self) {
        self.text = DialogString {
            text: self.name.clone(),
            padding: 0,
        };
    }

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
        // if self.name_vma_pointer != ptr_le {
        //     warn!(
        //         "Got {}, expected {}",
        //         encode_hex(&ptr_le.to_le_bytes()),
        //         encode_hex(&self.name_vma_pointer.to_le_bytes())
        //     );
        // }
        self.name_vma_pointer = ptr_le;
    }
}

#[inline]
pub async fn parse<R: AsyncBufRead + AsyncSeek + Unpin>(
    reader: &mut R,
    pointers: &mut Vec<u32>,
) -> Result<BTreeMap<Hexu32, ItemInfo>, io::Error> {
    reader
        .seek(SeekFrom::Start(ITEM_STRUCTS_START as u64))
        .await
        .unwrap();
    let mut field_bytes = [0u8; 4];
    let mut field_vec = Vec::with_capacity(ITEM_STRUCT_FIELDS);
    let mut items = BTreeMap::new();
    for item_no in 0..ITEM_STRUCT_COUNT {
        for _field_no in 0..ITEM_STRUCT_FIELDS {
            reader.read_exact(&mut field_bytes).await?;
            field_vec.push(field_bytes);
        }
        field_vec.reverse();
        let pointer = field_vec.pop().unwrap();
        let name_vma_pointer = u32::from_le_bytes(pointer);
        pointers.push(name_vma_pointer);
        let slot_data = field_vec.pop().unwrap();
        let slot_val = i16::from_le_bytes([slot_data[0], slot_data[1]]);
        let unknown_2 = u16::from_le_bytes([slot_data[2], slot_data[3]]);
        let buy_price = i32::from_le_bytes(field_vec.pop().unwrap());
        let sell_price = i32::from_le_bytes(field_vec.pop().unwrap());
        let attributes = field_vec.pop().unwrap();
        let at_de = field_vec.pop().unwrap();
        let sk_ag = field_vec.pop().unwrap();
        let lu_f4 = field_vec.pop().unwrap();
        let character_equip_byte = attributes[0];
        let unknown_5 = attributes[1];
        let enchantment = Enchant::from_byte(attributes[2]);
        let important = attributes[3] & 0x40 == 0x40;
        // let attributes = u16::from_le_bytes([eq_f3[2], eq_f3[3]]);
        let attack = i16::from_le_bytes([at_de[0], at_de[1]]);
        let defense = i16::from_le_bytes([at_de[2], at_de[3]]);
        let skill = i16::from_le_bytes([sk_ag[0], sk_ag[1]]);
        let agility = i16::from_le_bytes([sk_ag[2], sk_ag[3]]);
        let luck = i16::from_le_bytes([lu_f4[0], lu_f4[1]]);
        let unknown_7 = u16::from_le_bytes([lu_f4[2], lu_f4[3]]);
        let equip_slot = ItemEquipSlot::try_from(slot_val).unwrap();

        let item = ItemInfo {
            name: Vec::new(),
            name_vma_pointer,
            text: DialogString::default(),
            relative_name_pointer: RelativePointerInfo::default(),
            equip_slot,
            unknown_2,
            buy_price,
            sell_price,
            unknown_5,
            enchantment,
            important,
            // attributes: u32::from_le_bytes(attributes),
            can_equip: Character::from_byte(character_equip_byte),
            attack,
            defense,
            skill,
            agility,
            luck,
            unknown_7,
        };
        items.insert(Hexu32(u32::try_from(item_no).unwrap()), item);
    }
    // use crate::slpm_patcher::Hexu32;
    // use alloc::collections::BTreeMap;
    // use crate::helpers::{save_binary_file, encode_hex};
    // use std::path::PathBuf;
    // let mut item_pointers = BTreeMap::new();
    for item in items.values_mut() {
        let ptr = item.name_vma_pointer;
        reader
            .seek(SeekFrom::Start(u64::from(ptr - POINTER_OFFSET)))
            .await
            .unwrap();
        let mut string_bytes = Vec::with_capacity(20);
        while let Ok(byte) = reader.read_u8().await
            && byte != 0
        {
            string_bytes.push(byte);
        }
        let engrish_str = decode_psg2_string(string_bytes).text;
        item.name = engrish_str;
        // item_pointers.insert(
        //     Hexu32(item.name_pointer - 0xff000),
        //     (
        //         encode_hex(&item.name_pointer.to_le_bytes()),
        //         item.item_name.to_string(),
        //     ),
        // );
    }
    // let bytes = serde_json::to_string_pretty(&item_pointers)
    //     .unwrap()
    //     .into_bytes();
    // save_binary_file(&PathBuf::from("eng_item_pointers.json"), &bytes).await?;
    Ok(items)
}

#[inline]
pub async fn patch(
    exec_writer: &mut BufWriter<fs::File>,
    items: BTreeMap<Hexu32, ItemInfo>,
) -> Result<(), io::Error> {
    exec_writer
        .seek(SeekFrom::Start(ITEM_STRUCTS_START as u64))
        .await
        .unwrap();
    assert_eq!(ITEM_STRUCT_COUNT, items.len(), "Item count MUST be exact!");
    for (_, item) in items {
        // Calculate attributes field
        let equip_byte = Character::to_byte(&item.can_equip);
        let padding = item.unknown_5;
        let enchant_byte = Enchant::to_byte(&item.enchantment);
        let important = if item.important { 0x40 } else { 0x00 };

        // Now write it all
        exec_writer
            .write_all(&item.name_vma_pointer.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&(item.equip_slot as i16).to_le_bytes())
            .await?;
        exec_writer.write_all(&item.unknown_2.to_le_bytes()).await?;
        exec_writer.write_all(&item.buy_price.to_le_bytes()).await?;
        exec_writer
            .write_all(&item.sell_price.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&[equip_byte, padding, enchant_byte, important])
            .await?;
        exec_writer.write_all(&item.attack.to_le_bytes()).await?;
        exec_writer.write_all(&item.defense.to_le_bytes()).await?;
        exec_writer.write_all(&item.skill.to_le_bytes()).await?;
        exec_writer.write_all(&item.agility.to_le_bytes()).await?;
        exec_writer.write_all(&item.luck.to_le_bytes()).await?;
        exec_writer.write_all(&item.unknown_7.to_le_bytes()).await?;
    }
    Ok(())
}

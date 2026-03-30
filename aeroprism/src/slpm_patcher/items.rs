use crate::{
    events::{
        DialogItem, codec::decode_psg2_string, deserialize_dialog_items, serialize_dialog_items,
    },
    helpers::{
        deserialize_u8_hex, deserialize_u16_hex, deserialize_u32_hex, is_default, is_u16_max,
        max_u16, serialize_u8_hex, serialize_u16_hex, serialize_u32_hex,
    },
    slpm_patcher::{Character, Hexu32, ItemElemental, POINTER_OFFSET},
};
use alloc::collections::BTreeMap;
use core::mem::size_of;
use serde::{Deserialize, Serialize};
use std::io;
use tokio::{
    fs::{self},
    io::{AsyncBufRead, AsyncReadExt, AsyncSeek, AsyncSeekExt, AsyncWriteExt, BufWriter, SeekFrom},
};

static ITEM_STRUCTS_START: usize = 0x18_B1D0;
static ITEM_STRUCT_SIZE: usize = 32;
static ITEM_STRUCT_COUNT: usize = 185;
static ITEM_STRUCT_FIELDS: usize = ITEM_STRUCT_SIZE / size_of::<u32>();

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

#[derive(Serialize, Deserialize)]
pub struct ItemInfo {
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
    #[serde(default, skip_serializing_if = "is_default")]
    equip_slot: ItemEquipSlot,
    #[serde(
        default,
        serialize_with = "serialize_u8_hex",
        deserialize_with = "deserialize_u8_hex",
        skip_serializing_if = "is_default"
    )]
    field_1: u8, // Appears unused
    #[serde(
        default = "max_u16",
        serialize_with = "serialize_u16_hex",
        deserialize_with = "deserialize_u16_hex",
        skip_serializing_if = "is_u16_max"
    )]
    field_2: u16, // Appears unused
    #[serde(default, skip_serializing_if = "is_default")]
    buy_price: u32,
    #[serde(default, skip_serializing_if = "is_default")]
    sell_price: u32,
    #[serde(default, skip_serializing_if = "is_default")]
    can_equip: Box<[Character]>,
    #[serde(
        default,
        serialize_with = "serialize_u8_hex",
        deserialize_with = "deserialize_u8_hex",
        skip_serializing_if = "is_default"
    )]
    field_5: u8, // Appears unused
    #[serde(default, skip_serializing_if = "is_default")]
    elemental: Box<[ItemElemental]>,
    #[serde(
        default,
        serialize_with = "serialize_u16_hex",
        deserialize_with = "deserialize_u16_hex",
        skip_serializing_if = "is_default"
    )]
    attributes: u16,
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
    field_7: u16, // Appears unused
}

#[inline]
pub async fn parse<R: AsyncBufRead + AsyncSeek + Unpin>(
    reader: &mut R,
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
        let name_pointer = u32::from_le_bytes(pointer) - u32::try_from(POINTER_OFFSET).unwrap();
        let name_vma_pointer = u32::from_be_bytes(pointer);
        let slot_data = field_vec.pop().unwrap();
        let slot_byte = slot_data[0];
        let field_1 = slot_data[1];
        let field_2 = u16::from_le_bytes([slot_data[2], slot_data[3]]);
        let buy_price = u32::from_le_bytes(field_vec.pop().unwrap());
        let sell_price = u32::from_le_bytes(field_vec.pop().unwrap());
        let eq_f3 = field_vec.pop().unwrap();
        let at_de = field_vec.pop().unwrap();
        let sk_ag = field_vec.pop().unwrap();
        let lu_f4 = field_vec.pop().unwrap();
        let character_byte = eq_f3[0];
        let field_5 = eq_f3[1];
        let elemental = ItemElemental::multi_from_byte(eq_f3[2]);
        let attributes = u16::from_le_bytes([eq_f3[2], eq_f3[3]]);
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

        let item = ItemInfo {
            name: Vec::new(),
            name_pointer,
            name_vma_pointer,
            equip_slot,
            field_1,
            field_2,
            buy_price,
            sell_price,
            field_5,
            elemental,
            attributes,
            can_equip: Character::character_list_from_byte(character_byte),
            attack,
            defense,
            skill,
            agility,
            luck,
            field_7,
        };
        items.insert(Hexu32(u32::try_from(item_no + 1).unwrap()), item);
    }
    // use crate::slpm_patcher::Hexu32;
    // use alloc::collections::BTreeMap;
    // use crate::helpers::{save_binary_file, encode_hex};
    // use std::path::PathBuf;
    // let mut item_pointers = BTreeMap::new();
    for item in items.values_mut() {
        reader
            .seek(SeekFrom::Start(u64::from(item.name_pointer)))
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
        .seek(SeekFrom::Start(ITEM_STRUCTS_START.try_into().unwrap()))
        .await
        .unwrap();
    assert_eq!(ITEM_STRUCT_COUNT, items.len(), "Item count MUST be exact!");
    for (_, item) in items {
        let mut equip_byte = 0;
        for character in item.can_equip {
            equip_byte |= character as u8;
        }
        exec_writer
            .write_all(&item.name_vma_pointer.to_be_bytes())
            .await?;
        exec_writer.write_u8(item.equip_slot as u8).await?;
        exec_writer.write_u8(item.field_1).await?;
        exec_writer.write_all(&item.field_2.to_le_bytes()).await?;
        exec_writer.write_all(&item.buy_price.to_le_bytes()).await?;
        exec_writer
            .write_all(&item.sell_price.to_le_bytes())
            .await?;
        exec_writer.write_u8(equip_byte).await?;
        exec_writer.write_u8(item.field_5).await?;
        exec_writer
            .write_all(&item.attributes.to_le_bytes())
            .await?;
        exec_writer.write_all(&item.attack.to_le_bytes()).await?;
        exec_writer.write_all(&item.defense.to_le_bytes()).await?;
        exec_writer.write_all(&item.skill.to_le_bytes()).await?;
        exec_writer.write_all(&item.agility.to_le_bytes()).await?;
        exec_writer.write_all(&item.luck.to_le_bytes()).await?;
        exec_writer.write_all(&item.field_7.to_le_bytes()).await?;
    }
    Ok(())
}

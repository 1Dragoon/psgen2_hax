#![allow(clippy::arbitrary_source_item_ordering, reason = "not needed")]
use crate::{
    events::{
        DialogString,
        codec::{decode_psg2_string, parse_next_event_char},
    },
    helpers::{
        deserialize_u8_hex, deserialize_u16_hex, deserialize_u32_hex, hex_edit_encode,
        serialize_u8_hex, serialize_u16_hex, serialize_u32_hex,
    },
};
use core::{mem::size_of, panic};
use log::{Level, debug, log_enabled, warn};
use serde::{Deserialize, Serialize};
use std::io;
use tokio::{
    fs,
    io::{
        AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncSeek, AsyncSeekExt, AsyncWriteExt,
        BufWriter, SeekFrom,
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
    pub items: Vec<ItemInfo>,
    pub enemies: Vec<EnemyInfo>,
    // pub strings: Vec<String>,
    pub end_credits: Vec<EndCreditItem>,
}

#[derive(Serialize, Deserialize, Default, PartialEq)]
enum ItemEquipSlot {
    OneHand,
    TwoHand,
    Head,
    Shield,
    Torso,
    Feet,
    #[default]
    None,
}

#[derive(Serialize, Deserialize)]
enum Character {
    Eusis,
    Nei,
    Rudger,
    Anne,
    Huey,
    Amia,
    Keinz,
    Silka,
}

fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    can_equip: Vec<Character>,
    #[serde(
        default,
        serialize_with = "serialize_u16_hex",
        deserialize_with = "deserialize_u16_hex",
        skip_serializing_if = "is_default"
    )]
    field_3: u16,
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
    #[serde(default, skip_serializing_if = "is_default")]
    field_4: i16,
}

#[derive(Serialize, Deserialize)]
enum Elemental {
    Fire,      // Bitmask: 0x01
    Ice,       // Bitmask: 0x02
    Air,       // Bitmask: 0x04
    Lightning, // Bitmask: 0x08
}

#[derive(Serialize, Deserialize, Default)]
enum EnemyType {
    Biologic, // Bitmask: 0x01
    Robotic,  // Bitmask: 0x02
    #[default]
    Demonic, // First and second bits turned off. Effectively, the above two bits count as a weakness to certain techniques. This simply indicates immunity to both biologic and robitic techniques.
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
    #[serde(
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex"
    )]
    attribute_field: u32, // All bytes of the attribute field. The below values will overwrite the data in this field if it is changed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    resistances: Vec<Elemental>, // First four bits of first byte of attribute field
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    weaknesses: Vec<Elemental>, // Second four bits of first byte of attribute field
    r#type: EnemyType, // First four bits of third byte of attribute field
    #[serde(default, skip_serializing_if = "is_default")]
    boss: bool, // Mask: 0x04. The name is just a guess. Possessed by Dark Falz, Motherbrain, Neifirst (both occurrences) and Army Eye. No idea what it does.
    #[serde(default, skip_serializing_if = "is_default")]
    super_boss: bool, // Mask: 0x08. As above, the name is just a guess. Only Dark Falz and Motherbrain appear to have the bit for this set. As above, no idea what it does.
    #[serde(default, skip_serializing_if = "is_default")]
    field_5: u8, // Second four bits of third byte of attribute field. No idea what it does
    #[serde(
        default,
        serialize_with = "serialize_u8_hex",
        deserialize_with = "deserialize_u8_hex",
        skip_serializing_if = "is_default"
    )]
    animation: u8, // Fourth byte of attribute field. Controls graphical effects such as whether the enemy floats, sits still, flashes, and others.
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

pub async fn parse_enemies<R: AsyncBufRead + AsyncSeek + Unpin>(reader: &mut R) -> Vec<EnemyInfo> {
    reader
        .seek(SeekFrom::Start(ENEMY_STRUCTS_START as u64))
        .await
        .unwrap();
    let mut field_bytes = [0u8; 4];
    let mut field_vec = Vec::with_capacity(ENEMY_STRUCT_FIELDS);
    let mut enemies = Vec::with_capacity(ENEMY_STRUCT_COUNT);
    for enemy_no in 0..ENEMY_STRUCT_COUNT {
        for _field_no in 0..ENEMY_STRUCT_FIELDS {
            reader.read_exact(&mut field_bytes).await.unwrap();
            field_vec.push(field_bytes);
        }
        field_vec.reverse();
        let mut enemy = EnemyInfo {
            enemy_number: enemy_no + 1usize,
            enemy_name: String::new(),
            name_pointer: u32::from_le_bytes(field_vec.pop().unwrap()),
            attribute_field: u32::from_be_bytes(field_vec.pop().unwrap()),
            resistances: Vec::with_capacity(2),
            weaknesses: Vec::with_capacity(2),
            field_5: 0,
            r#type: EnemyType::default(),
            animation: 0,
            boss: false,
            super_boss: false,
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
        let attr_field = enemy.attribute_field.to_be_bytes();
        let resistances = attr_field[0] >> 4;
        if resistances & 0x1 == 0x1 {
            enemy.resistances.push(Elemental::Fire);
        }
        if resistances & 0x2 == 0x2 {
            enemy.resistances.push(Elemental::Ice);
        }
        if resistances & 0x4 == 0x4 {
            enemy.resistances.push(Elemental::Air);
        }
        if resistances & 0x8 == 0x8 {
            enemy.resistances.push(Elemental::Lightning);
        }
        let weaknesses = attr_field[0] & 0xf;
        if weaknesses & 0x1 == 0x1 {
            enemy.weaknesses.push(Elemental::Fire);
        }
        if weaknesses & 0x2 == 0x2 {
            enemy.weaknesses.push(Elemental::Ice);
        }
        if weaknesses & 0x4 == 0x4 {
            enemy.weaknesses.push(Elemental::Air);
        }
        if weaknesses & 0x8 == 0x8 {
            enemy.weaknesses.push(Elemental::Lightning);
        }
        enemy.field_5 = attr_field[1];
        let enemy_types = attr_field[2] >> 4;
        if enemy_types & 0x1 == 0x1 {
            enemy.r#type = EnemyType::Biologic;
        } else if enemy_types & 0x2 == 0x2 {
            enemy.r#type = EnemyType::Robotic;
        } else if enemy_types & 0x3 == 0x3 {
            #[expect(clippy::panic, reason = "Just a sanity check.")]
            {
                panic!("Enemy flagged as both biologic AND robitic! This is invalid.")
            }
        }
        // #[expect(clippy::verbose_bit_mask, reason = "Readability.")]
        // if enemy_types & 0x3 == 0x0 {
        //     enemy.r#type.push(EnemyType::Demonic);
        // }
        if enemy_types & 0x4 == 0x4 {
            enemy.boss = true;
        }
        if enemy_types & 0x8 == 0x8 {
            enemy.super_boss = true;
        }
        enemy.animation = attr_field[3];
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
        reader.read_until(0, &mut string_bytes).await.unwrap();
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
    enemies
}

pub async fn parse_items<R: AsyncBufRead + AsyncSeek + Unpin>(reader: &mut R) -> Vec<ItemInfo> {
    reader
        .seek(SeekFrom::Start(ITEM_STRUCTS_START as u64))
        .await
        .unwrap();
    let mut field_bytes = [0u8; 4];
    let mut field_vec = Vec::with_capacity(ITEM_STRUCT_FIELDS);
    let mut items = Vec::with_capacity(ITEM_STRUCT_COUNT);
    for item_no in 0..ITEM_STRUCT_COUNT {
        for _field_no in 0..ITEM_STRUCT_FIELDS {
            reader.read_exact(&mut field_bytes).await.unwrap();
            field_vec.push(field_bytes);
        }
        field_vec.reverse();
        let name_pointer = u32::from_le_bytes(field_vec.pop().unwrap());
        let slot_data = field_vec.pop().unwrap()[0];
        let field_1 = u32::from_le_bytes(field_vec.pop().unwrap());
        let field_2 = u32::from_le_bytes(field_vec.pop().unwrap());
        let eq_f3 = field_vec.pop().unwrap();
        let at_de = field_vec.pop().unwrap();
        let sk_ag = field_vec.pop().unwrap();
        let lu_f4 = field_vec.pop().unwrap();
        let equip_by = i16::from_le_bytes([eq_f3[0], eq_f3[1]]);
        let field_3 = u16::from_le_bytes([eq_f3[2], eq_f3[3]]);
        let attack = i16::from_le_bytes([at_de[0], at_de[1]]);
        let defense = i16::from_le_bytes([at_de[2], at_de[3]]);
        let skill = i16::from_le_bytes([sk_ag[0], sk_ag[1]]);
        let agility = i16::from_le_bytes([sk_ag[2], sk_ag[3]]);
        let luck = i16::from_le_bytes([lu_f4[0], lu_f4[1]]);
        let field_4 = i16::from_le_bytes([lu_f4[2], lu_f4[3]]);

        let equip_slot = match slot_data {
            1 => ItemEquipSlot::OneHand,
            2 => ItemEquipSlot::TwoHand,
            3 => ItemEquipSlot::Head,
            4 => ItemEquipSlot::Shield,
            5 => ItemEquipSlot::Torso,
            6 => ItemEquipSlot::Feet,
            _ => ItemEquipSlot::None,
        };

        let mut can_equip = Vec::with_capacity(8);
        if equip_by & 0x01 == 0x01 {
            can_equip.push(Character::Eusis);
        }
        if equip_by & 0x02 == 0x02 {
            can_equip.push(Character::Nei);
        }
        if equip_by & 0x04 == 0x04 {
            can_equip.push(Character::Rudger);
        }
        if equip_by & 0x08 == 0x08 {
            can_equip.push(Character::Anne);
        }
        if equip_by & 0x10 == 0x10 {
            can_equip.push(Character::Huey);
        }
        if equip_by & 0x20 == 0x20 {
            can_equip.push(Character::Amia);
        }
        if equip_by & 0x40 == 0x40 {
            can_equip.push(Character::Keinz);
        }
        if equip_by & 0x80 == 0x80 {
            can_equip.push(Character::Silka);
        }

        let item = ItemInfo {
            item_number: u32::try_from(item_no + 1).unwrap(),
            item_name: DialogString::default(),
            name_pointer,
            equip_slot,
            field_1,
            field_2,
            field_3,
            can_equip,
            attack,
            defense,
            skill,
            agility,
            luck,
            field_4,
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
        reader.read_until(0, &mut string_bytes).await.unwrap();
        let engrish_str = decode_psg2_string(string_bytes);
        item.item_name = engrish_str;
        // let mut string_bytes_iter = string_bytes.into_iter().peekable();
        // let mut engrish_str = Vec::with_capacity(20);
        // while let Some(byte) = string_bytes_iter.next()
        //     && byte != 0
        // {
        //     if let Err(SjisError::UnexpectedCharacter { byte: unexpected }) =
        //         parse_next_event_char(&mut string_bytes_iter, &mut engrish_str, byte)
        //     {
        //         engrish_str.push(format!("\\x{unexpected:02x}"));
        //     }
        // }
        // item.item_name = engrish_str.concat();
        // item.item_name.shrink_to_fit();
    }
    items.shrink_to_fit();
    items
}

// pub async fn parse_map_strings<R: AsyncBufRead + AsyncSeek + Unpin>(reader: &mut R) -> Vec<String> {
//     reader
//         .seek(SeekFrom::Start(MAPNAMES_JUMPLIST_START as u64))
//         .await
//         .unwrap();
//     let mut pointer_bytes = [0u8; 4];
//     let mut pointer_vec = Vec::with_capacity(MAPNAMES_POINTER_COUNT);
//     let mut mapnames = Vec::with_capacity(MAPNAMES_POINTER_COUNT);
//     for _ in 0..MAPNAMES_POINTER_COUNT {
//         reader.read_exact(&mut pointer_bytes).await.unwrap();
//         pointer_vec.push(u32::from_le_bytes(pointer_bytes));
//     }
//     for pointer in pointer_vec {
//         reader
//             .seek(SeekFrom::Start(u64::from(pointer) - POINTER_OFFSET as u64))
//             .await
//             .unwrap();
//         let mut string_bytes = Vec::with_capacity(20);
//         reader.read_until(0, &mut string_bytes).await.unwrap();
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
//     mapnames
// }

#[derive(Serialize, Deserialize)]
pub struct EndCreditItem {
    vertical_space: u16,
    credit_string: DialogString,
}

pub async fn parse_end_credits<R: AsyncBufRead + AsyncSeek + Unpin>(
    reader: &mut R,
) -> Vec<EndCreditItem> {
    reader
        .seek(SeekFrom::Start(END_CREDITS_START as u64))
        .await
        .unwrap();

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
        reader.read_exact(&mut field).await.unwrap();
        // Read the vertical space number
        reader.read_exact(&mut field).await.unwrap();
        let vertical_space = u16::from_le_bytes(field);
        // Read the 0x0200 marker
        reader.read_exact(&mut field).await.unwrap();
        let second_marker = u16::from_le_bytes(field);
        if second_marker != 2 {
            // If the 0x0200 marker is 0x0000, that is the signal to display the "THE END" graphic after scrolling the
            // vertical space distance in the final header.
            debug!("Ended on {i}th credit.");
            break;
        }
        // Move the cursor past the length indicator -- we only need to calculate it dynamically upon patching.
        reader.read_exact(&mut field).await.unwrap();

        // Read all of the bytes until the first null terminator
        let mut string_bytes = Vec::with_capacity(32);
        while let Ok(byte) = reader.read_u8().await
            && byte != 0
        {
            string_bytes.push(byte);
        }
        // Move the cursor to the start of the next field

        while reader.stream_position().await.unwrap() % 4 != 0 {
            reader.read_u8().await.unwrap();
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
    // reader.read_exact(&mut credit_bytes).await.unwrap();
    // let mut credits_iter
    credit_items.shrink_to_fit();
    credit_items
}

pub async fn patch_end_credits(
    exec_writer: &mut BufWriter<fs::File>,
    end_credits: Vec<EndCreditItem>,
) -> Result<(), io::Error> {
    exec_writer
        .seek(SeekFrom::Start(END_CREDITS_START.try_into().unwrap()))
        .await
        .unwrap();
    let mut total_bytes = 0;
    for item in end_credits {
        let EndCreditItem {
            vertical_space,
            credit_string,
        } = item;

        if log_enabled!(Level::Debug) {
            debug!("Debugged credit string: {credit_string:#?}");
        }

        // Convert the string into bytes and calculate the length field, storing as a u16 for later
        let mut string_bytes = credit_string.into_bytes(None);
        string_bytes.push(0);
        let string_length = u16::try_from(string_bytes.len()).unwrap();
        // Add padding
        while !(string_bytes.len()).is_multiple_of(4) {
            string_bytes.push(0);
        }

        if log_enabled!(Level::Debug) {
            debug!("Rendered credit string: {}", hex_edit_encode(&string_bytes));
        }

        let credit_header = [
            1u16.to_le_bytes(),
            vertical_space.to_le_bytes(),
            2u16.to_le_bytes(),
            string_length.to_le_bytes(),
        ]
        .concat();

        let expand_by = string_bytes.len() + credit_header.len();
        if expand_by + total_bytes + CREDIT_ITEM_HEADER_SIZE >= END_CREDITS_BLOB_SIZE {
            if log_enabled!(Level::Warn) {
                warn!(
                    "End credit overflow! Data corruption likely! Overflowed by {} bytes",
                    END_CREDITS_BLOB_SIZE.saturating_sub(expand_by + total_bytes)
                );
            }
            break;
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

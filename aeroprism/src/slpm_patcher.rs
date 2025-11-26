use crate::{
    events::codec::{SjisError, parse_next_sjis},
    helpers::{deserialize_u32_hex, serialize_u32_hex},
};
use core::mem::size_of;
use serde::{Deserialize, Serialize};
use std::io::{BufRead, Seek};
use tokio::{
    fs::File,
    io::{
        AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncSeek, AsyncSeekExt, AsyncWriteExt,
        BufWriter, SeekFrom,
    },
};

// Via `readelf d:\SLPM_625.53 -l`
// Program segment 1 and 2 both end up as 0xFF000
static VMA_OFFSET: usize = 0x10_0000;
static FILE_OFFSET: usize = 0x1000;
static POINTER_OFFSET: usize = VMA_OFFSET - FILE_OFFSET;

static ENEMY_STRUCTS_START: usize = 0x1A_422C;
static ENEMY_STRUCT_SIZE: usize = 148;
static ENEMY_STRUCT_COUNT: usize = 124;
static ENEMY_STRUCT_FIELDS: usize = ENEMY_STRUCT_SIZE / size_of::<u32>();

static ITEM_STRUCTS_START: usize = 0x18_B1D0;
static ITEM_STRUCT_SIZE: usize = 32;
static ITEM_STRUCT_COUNT: usize = 185;
static ITEM_STRUCT_FIELDS: usize = ITEM_STRUCT_SIZE / size_of::<u32>();

static MAPNAMES_JUMPLIST_START: usize = 0x14_F798;
static MAPNAMES_POINTER_COUNT: usize = 106;

#[derive(Serialize, Deserialize)]
struct MapNameVec(Vec<String>);

#[derive(Serialize, Deserialize)]
struct ItemData {
    #[serde(
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex"
    )]
    item_number: u32,
    item_name: String,
    #[serde(
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex"
    )]
    name_pointer: u32,
    field_1: u32,
    field_2: u32,
    field_3: u32,
    field_4: u32,
    field_5: u32,
    field_6: u32,
    field_7: u32,
}

#[derive(Serialize, Deserialize)]
struct ItemDataVec(Vec<ItemData>);

#[derive(Serialize, Deserialize)]
enum Elemental {
    Fire,      // 0x01
    Ice,       // 0x02
    Air,       // 0x04
    Lightning, // 0x08
}

#[derive(Serialize, Deserialize)]
enum EnemyType {
    Biologic,  // 0x01
    Robotic,   // 0x02
    Demonic, // First and second bits turned off. Effectively, the above two bits count as a weakness to certain techniques. This simply indicates no weakness.
    Boss, // 0x04. The name is just a guess. Possessed by Dark Falz, Motherbrain, Neifirst (both occurrences) and Army Eye. No idea what it does.
    SuperBoss, // 0x08. As above, the name is just a guess. Only Dark Falz and Motherbrain appear to have the bit for this set. As above, no idea what it does.
}

#[derive(Serialize, Deserialize)]
struct EnemyData {
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
    attribute_field: u32, // All bytes of the second field. The below values will overwrite the data in this field if it is changed.
    resistances: Vec<Elemental>, // First four bits of first byte of second field
    weaknesses: Vec<Elemental>,  // Second four bits of first byte of second field
    // No idea what the second byte does -- most likely nothing
    r#type: Vec<EnemyType>, // First four bits of third byte of second field
    // Fourth byte controls graphical effects such as whether the enemy floats, sits still, flashes, and others.
    health: u32,  // Third field
    attack: u32,  // Fourth field
    defense: u32, // Fifth field
    agility: u32, // Sixth field. Controls chance to dodge your hits, possibly others.
    field_7: u32, // These fields serve an unknown purpose. Possible values include: technique power, technique points
    field_8: u32,
    field_9: u32,
    field_10: u32,
    field_11: u32,
    // 12 through 17 appear to control the art assets used for this enemy. E.g. dropping the data in these fields from mother brain into neifirst will make neifirst look like mother brain
    art_1: u32, // Field 12
    art_2: u32,
    art_3: u32,
    art_4: u32,
    art_5: u32,
    art_6: u32, // Field 17
    field_18: u32,
    field_19: u32,
    field_20: u32,
    field_21: u32,
    field_22: u32,
    field_23: u32,
    field_24: u32,
    field_25: u32,
    field_26: u32,
    field_27: u32,
    field_28: u32,
    field_29: u32,
    field_30: u32,
    field_31: u32,
    field_32: u32,
    field_33: u32,
    field_34: u32,
    field_35: u32,
    field_36: u32,
    field_37: u32,
}

#[derive(Serialize, Deserialize)]
struct EnemyDataVec(Vec<EnemyData>);

pub async fn parse_enemies<R: AsyncBufRead + AsyncSeek + Unpin>(reader: &mut R) {
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
        let mut enemy = EnemyData {
            enemy_number: enemy_no + 1usize,
            enemy_name: String::new(),
            name_pointer: u32::from_le_bytes(field_vec.pop().unwrap()),
            attribute_field: u32::from_be_bytes(field_vec.pop().unwrap()),
            resistances: vec![],
            weaknesses: vec![],
            r#type: vec![],
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
        let enemy_types = attr_field[2] >> 4;
        if enemy_types & 0x1 == 0x1 {
            enemy.r#type.push(EnemyType::Biologic);
        }
        if enemy_types & 0x2 == 0x2 {
            enemy.r#type.push(EnemyType::Robotic);
        }
        #[expect(clippy::verbose_bit_mask, reason = "Readability.")]
        if enemy_types & 0x3 == 0x0 {
            enemy.r#type.push(EnemyType::Demonic);
        }
        if enemy_types & 0x4 == 0x4 {
            enemy.r#type.push(EnemyType::Boss);
        }
        if enemy_types & 0x8 == 0x8 {
            enemy.r#type.push(EnemyType::SuperBoss);
        }
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
            if let Err(SjisError::UnexpectedCharacter { byte }) =
                parse_next_sjis(&mut string_bytes_iter, &mut engrish_str, byte)
            {
                engrish_str.push(format!("x{byte:02x}"));
            }
        }
        enemy.enemy_name = engrish_str.concat();
    }
    let enemy_data_string = serde_json::to_string_pretty(&EnemyDataVec(enemies)).unwrap();
    println!("{enemy_data_string}");
}

pub async fn parse_items<R: AsyncBufRead + AsyncSeek + Unpin>(reader: &mut R) {
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
        let item = ItemData {
            item_number: (item_no + 1) as u32,
            item_name: String::new(),
            name_pointer: u32::from_le_bytes(field_vec.pop().unwrap()),
            field_1: u32::from_le_bytes(field_vec.pop().unwrap()),
            field_2: u32::from_le_bytes(field_vec.pop().unwrap()),
            field_3: u32::from_le_bytes(field_vec.pop().unwrap()),
            field_4: u32::from_le_bytes(field_vec.pop().unwrap()),
            field_5: u32::from_le_bytes(field_vec.pop().unwrap()),
            field_6: u32::from_le_bytes(field_vec.pop().unwrap()),
            field_7: u32::from_le_bytes(field_vec.pop().unwrap()),
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
        let mut string_bytes_iter = string_bytes.into_iter().peekable();
        let mut engrish_str = Vec::with_capacity(20);
        while let Some(byte) = string_bytes_iter.next()
            && byte != 0
        {
            if let Err(SjisError::UnexpectedCharacter { byte }) =
                parse_next_sjis(&mut string_bytes_iter, &mut engrish_str, byte)
            {
                engrish_str.push(format!("x{byte:02x}"));
            }
        }
        item.item_name = engrish_str.concat();
    }
    let item_data_string = serde_json::to_string_pretty(&ItemDataVec(items)).unwrap();
    println!("{item_data_string}");
}

pub async fn parse_map_strings<R: AsyncBufRead + AsyncSeek + Unpin>(reader: &mut R) {
    reader
        .seek(SeekFrom::Start(MAPNAMES_JUMPLIST_START as u64))
        .await
        .unwrap();
    let mut pointer_bytes = [0u8; 4];
    let mut pointer_vec = Vec::with_capacity(MAPNAMES_POINTER_COUNT);
    let mut mapnames = Vec::with_capacity(MAPNAMES_POINTER_COUNT);
    for _ in 0..MAPNAMES_POINTER_COUNT {
        reader.read_exact(&mut pointer_bytes).await.unwrap();
        pointer_vec.push(u32::from_le_bytes(pointer_bytes))
    }
    for pointer in pointer_vec {
        reader
            .seek(SeekFrom::Start(
                u64::from(pointer) - POINTER_OFFSET as u64,
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
            if let Err(SjisError::UnexpectedCharacter { byte }) =
                parse_next_sjis(&mut string_bytes_iter, &mut engrish_str, byte)
            {
                engrish_str.push(format!("x{byte:02x}"));
            }
        }
        let mapname = engrish_str.concat();
        mapnames.push(mapname);
    }
    let map_names_string = serde_json::to_string_pretty(&MapNameVec(mapnames)).unwrap();
    println!("{map_names_string}");
}

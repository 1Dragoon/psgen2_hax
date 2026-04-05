use crate::{
    events::{DialogItem, codec::decode_psg2_string},
    helpers::is_default,
    slpm_patcher::{
        Hexu32, POINTER_OFFSET, SpellElemental, StringMemRegion, TECHNIQUE_STRUCT_COUNT,
        TECHNIQUE_STRUCT_FIELDS, TECHNIQUE_STRUCT_START,
    },
};
use alloc::collections::BTreeMap;
use indexmap::IndexSet;
use strum::EnumIter;
// use indexmap::IndexMap;
use crate::{
    events::{deserialize_dialog_items, serialize_dialog_items},
    helpers::{deserialize_u32_hex, serialize_u32_hex},
};
use serde::{Deserialize, Serialize};
use std::io;
use strum::IntoEnumIterator;
use tokio::io::{AsyncBufRead, AsyncReadExt, AsyncSeek, AsyncSeekExt, SeekFrom};

#[derive(Serialize, Deserialize)]
#[repr(i32)]
enum TargetOptions {
    SacrificeForAll = -7,
    SacrificeForOne = -6,
    AllyDead = -5,
    AllButCaster = -4,
    AllyAll = -3,
    Ally = -2,
    Caster = -1,
    Enemy = 0,
    EnemyGroup = 1,
    EnemyAll = 5,
}

impl TryFrom<i32> for TargetOptions {
    type Error = String;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            -7 => Ok(Self::SacrificeForAll),
            -6 => Ok(Self::SacrificeForOne),
            -5 => Ok(Self::AllyDead),
            -4 => Ok(Self::AllButCaster),
            -3 => Ok(Self::AllyAll),
            -2 => Ok(Self::Ally),
            -1 => Ok(Self::Caster),
            0 => Ok(Self::Enemy),
            1 => Ok(Self::EnemyGroup),
            5 => Ok(Self::EnemyAll),
            other => Err(format!("Invalid target value {other}")),
        }
    }
}

// Technique attributes
enum WhereUsed {
    BattleOffense = 0x04,
    BattleDefense = 0x02,
    Fastest = 0x10,
    Medium = 0x00,
    Slowest = 0x20,
}

// Technique field_2
enum TechEffect {
    InstaKill = 0x1000,
    MaxValue = 0x8000,
}

#[derive(Serialize, Deserialize)]
pub struct Technique {
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
    #[serde(
        default,
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex",
        skip_serializing_if = "is_default"
    )]
    attributes: u32,
    #[serde(default, skip_serializing_if = "is_default")]
    elemental: Box<[SpellElemental]>,
    #[serde(default, skip_serializing_if = "is_default")]
    vulnerable: Box<[Vulerabilities]>,
    #[serde(
        default,
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex",
        skip_serializing_if = "is_default"
    )]
    field_2: u32,
    #[serde(default, skip_serializing_if = "is_default")]
    tp_cost: u32,
    target: TargetOptions,
    #[serde(default, skip_serializing_if = "is_default")]
    power: i32,
    #[serde(default, skip_serializing_if = "is_default")]
    eusis: i32,
    #[serde(default, skip_serializing_if = "is_default")]
    nei: i32,
    #[serde(default, skip_serializing_if = "is_default")]
    rudger: i32,
    #[serde(default, skip_serializing_if = "is_default")]
    anne: i32,
    #[serde(default, skip_serializing_if = "is_default")]
    huey: i32,
    #[serde(default, skip_serializing_if = "is_default")]
    amia: i32,
    #[serde(default, skip_serializing_if = "is_default")]
    keinz: i32,
    #[serde(default, skip_serializing_if = "is_default")]
    silka: i32,
}

#[repr(u8)]
#[derive(EnumIter, Serialize, Deserialize, Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Debug)]
enum Vulerabilities {
    Biologic = 0x1,
    Robotic = 0x2,
    NotBoss = 0x4,
    NotSuperboss = 0x8,
}

impl Vulerabilities {
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

pub async fn parse<R: AsyncBufRead + AsyncSeek + Unpin>(
    reader: &mut R,
) -> Result<BTreeMap<Hexu32, Technique>, io::Error> {
    reader
        .seek(SeekFrom::Start(TECHNIQUE_STRUCT_START as u64))
        .await?;
    let mut field_bytes = [0u8; 4];
    let mut techniques = BTreeMap::new();
    let mut relative_pointer_index = IndexSet::with_capacity(TECHNIQUE_STRUCT_COUNT);
    for i in 0..TECHNIQUE_STRUCT_COUNT {
        let mut fields = Vec::with_capacity(TECHNIQUE_STRUCT_FIELDS);
        for _ in 0..TECHNIQUE_STRUCT_FIELDS {
            reader.read_exact(&mut field_bytes).await?;
            fields.push(field_bytes);
        }
        fields.reverse();
        let pointer_bytes = fields.pop().unwrap();
        let attributes = fields.pop().unwrap();
        let elemental = SpellElemental::from_byte(attributes[0] & 0xf);
        let vulnerable = Vulerabilities::from_byte(attributes[0] >> 4);
        let name_pointer =
            u32::from_le_bytes(pointer_bytes) - u32::try_from(POINTER_OFFSET).unwrap();
        relative_pointer_index.insert(name_pointer);

        let technique = Technique {
            name: Vec::new(),
            name_pointer,
            name_vma_pointer: u32::from_be_bytes(pointer_bytes),
            relative_name_pointer: (StringMemRegion::RegionA, Hexu32(0)),
            attributes: u32::from_le_bytes(attributes),
            elemental,
            vulnerable,
            field_2: u32::from_le_bytes(fields.pop().unwrap()),
            tp_cost: u32::from_le_bytes(fields.pop().unwrap()),
            target: TargetOptions::try_from(i32::from_le_bytes(fields.pop().unwrap())).unwrap(),
            power: i32::from_le_bytes(fields.pop().unwrap()),
            eusis: i32::from_le_bytes(fields.pop().unwrap()),
            nei: i32::from_le_bytes(fields.pop().unwrap()),
            rudger: i32::from_le_bytes(fields.pop().unwrap()),
            anne: i32::from_le_bytes(fields.pop().unwrap()),
            huey: i32::from_le_bytes(fields.pop().unwrap()),
            amia: i32::from_le_bytes(fields.pop().unwrap()),
            keinz: i32::from_le_bytes(fields.pop().unwrap()),
            silka: i32::from_le_bytes(fields.pop().unwrap()),
        };

        techniques.insert(Hexu32(u32::try_from(i).unwrap()), technique);
    }
    relative_pointer_index.sort_unstable();

    // let mut tech_pointers = BTreeMap::new();

    for technique in techniques.values_mut() {
        reader
            .seek(SeekFrom::Start(u64::from(technique.name_pointer)))
            .await?;
        let mut string_bytes = Vec::with_capacity(20);
        while let Ok(byte) = reader.read_u8().await
            && byte != 0
        {
            string_bytes.push(byte);
        }
        technique.name = decode_psg2_string(string_bytes).text;
        let region = StringMemRegion::try_from(technique.name_pointer).unwrap();
        let index = relative_pointer_index
            .get_index_of(&technique.name_pointer)
            .unwrap();
        technique.relative_name_pointer = (region, Hexu32(u32::try_from(index).unwrap()));
        // tech_pointers.insert(
        //     Hexu32(technique.name_pointer - 0xff000),
        //     (
        //         crate::helpers::encode_hex(&technique.name_pointer.to_le_bytes()),
        //         technique.name.to_string(),
        //     ),
        // );
    }
    // let bytes = serde_json::to_string_pretty(&tech_pointers)
    //     .unwrap()
    //     .into_bytes();
    // save_binary_file(&PathBuf::from("jap_tech_pointers.json"), &bytes).await?;

    Ok(techniques)
}

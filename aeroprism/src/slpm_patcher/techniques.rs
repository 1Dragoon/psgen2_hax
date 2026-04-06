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
use tokio::{
    fs,
    io::{AsyncBufRead, AsyncReadExt, AsyncSeek, AsyncSeekExt, AsyncWriteExt, BufWriter, SeekFrom},
};

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
#[derive(EnumIter, Serialize, Deserialize, Copy, Clone, Eq, PartialEq, Default)]
enum Speed {
    #[default]
    Medium = 0x00,
    Fast = 0x10,
    Slow = 0x20,
    SpeedA = 0x40,
    SpeedB = 0x80,
}

impl Speed {
    fn from_byte(mut byte: u8) -> Box<[Self]> {
        byte &= 0xf0;
        let mut variants = Vec::with_capacity(8);
        for variant in Self::iter() {
            if variant == Self::default() {
                continue;
            }
            if byte & variant as u8 == variant as u8 {
                variants.push(variant);
            }
        }
        if byte == 0 {
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



#[derive(EnumIter, Serialize, Deserialize, Copy, Clone, Eq, PartialEq, Default)]
enum Usage {
    #[default]
    EnemyEffect = 0x00,
    Caster = 0x01,
    Ally = 0x02,
    Enemy = 0x04,
    OtherA = 0x08,
}

impl Usage {
    fn from_byte(mut byte: u8) -> Box<[Self]> {
        byte &= 0x0f;
        let mut variants = Vec::with_capacity(8);
        for variant in Self::iter() {
            if variant == Self::default() {
                continue;
            }
            if byte & variant as u8 == variant as u8 {
                variants.push(variant);
            }
        }
        if byte == 0 {
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

// Technique field_2
#[derive(EnumIter, Serialize, Deserialize, Copy, Clone, Eq, PartialEq, Default)]
enum TechEffect {
    #[default]
    EffectA = 0x00,
    InstaKill = 0x10,
    EffectB = 0x20,
    EffectC = 0x40,
    MaxValue = 0x80,
}

impl TechEffect {
    fn from_byte(mut byte: u8) -> Box<[Self]> {
        byte &= 0xf0;
        let mut variants = Vec::with_capacity(8);
        for variant in Self::iter() {
            if variant == Self::default() {
                continue;
            }
            if byte & variant as u8 == variant as u8 {
                variants.push(variant);
            }
        }
        if byte == 0 {
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
    // #[serde(
    //     default,
    //     serialize_with = "serialize_u32_hex",
    //     deserialize_with = "deserialize_u32_hex",
    //     skip_serializing_if = "is_default"
    // )]
    // attributes: u32,
    #[serde(default, skip_serializing_if = "is_default")]
    elemental: Box<[SpellElemental]>,
    #[serde(default, skip_serializing_if = "is_default")]
    vulnerable: Box<[VulerableTargets]>,
    #[serde(default, skip_serializing_if = "is_default")]
    usage: Box<[Usage]>,
    #[serde(default, skip_serializing_if = "is_default")]
    speed: Box<[Speed]>,
    #[serde(default, skip_serializing_if = "is_default")]
    attr_c: u8,
    #[serde(
        default,
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex",
        skip_serializing_if = "is_default"
    )]
    field_2: u32,
    #[serde(default, skip_serializing_if = "is_default")]
    usable_out_of_combat: bool,
    #[serde(default, skip_serializing_if = "is_default")]
    special_effect: bool,
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
enum VulerableTargets {
    Biologic = 0x10,
    Robotic = 0x20,
    NotBoss = 0x40,
    NotSuperboss = 0x80,
}

impl VulerableTargets {
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
        let elemental = SpellElemental::from_byte(attributes[0]);
        let vulnerable = VulerableTargets::from_byte(attributes[0]);
        let name_pointer =
            u32::from_le_bytes(pointer_bytes) - u32::try_from(POINTER_OFFSET).unwrap();
        relative_pointer_index.insert(name_pointer);

        let usage = Usage::from_byte(attributes[1]);
        let speed = Speed::from_byte(attributes[1]);
        let attr_c = attributes[2];
        let attr_d = attributes[3];

        let usable_out_of_combat = attr_d & 0x80 == 0x80;
        let special_effect = attr_d & 0x40 == 0x40;
        // let attributes = u32::from_le_bytes(attributes);
        let field_2 = u32::from_le_bytes(fields.pop().unwrap());
        let tp_cost = u32::from_le_bytes(fields.pop().unwrap());
        let target = TargetOptions::try_from(i32::from_le_bytes(fields.pop().unwrap())).unwrap();
        let power = i32::from_le_bytes(fields.pop().unwrap());
        let eusis = i32::from_le_bytes(fields.pop().unwrap());
        let nei = i32::from_le_bytes(fields.pop().unwrap());
        let rudger = i32::from_le_bytes(fields.pop().unwrap());
        let anne = i32::from_le_bytes(fields.pop().unwrap());
        let huey = i32::from_le_bytes(fields.pop().unwrap());
        let amia = i32::from_le_bytes(fields.pop().unwrap());
        let keinz = i32::from_le_bytes(fields.pop().unwrap());
        let silka = i32::from_le_bytes(fields.pop().unwrap());

        let technique = Technique {
            name: Vec::new(),
            name_pointer,
            name_vma_pointer: u32::from_be_bytes(pointer_bytes),
            relative_name_pointer: (StringMemRegion::RegionA, Hexu32(0)),
            elemental,
            vulnerable,
            speed,
            usage,
            attr_c,
            usable_out_of_combat,
            special_effect,
            // attributes,
            field_2,
            tp_cost,
            target,
            power,
            eusis,
            nei,
            rudger,
            anne,
            huey,
            amia,
            keinz,
            silka,
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

#[inline]
pub async fn patch(
    exec_writer: &mut BufWriter<fs::File>,
    techniques: BTreeMap<Hexu32, Technique>,
) -> Result<(), io::Error> {
    exec_writer
        .seek(SeekFrom::Start(TECHNIQUE_STRUCT_START as u64))
        .await?;
    assert_eq!(
        TECHNIQUE_STRUCT_COUNT,
        techniques.len(),
        "Technique count MUST be exact!"
    );
    for (_, tech) in techniques {
        // Calculate attributes field
        let attr_a = SpellElemental::to_byte(&tech.elemental) | VulerableTargets::to_byte(&tech.vulnerable);
        let attr_b = Speed::to_byte(&tech.speed) | Usage::to_byte(&tech.usage);
        let attr_c = tech.attr_c;
        let mut attr_d = 0;
        if tech.usable_out_of_combat {
            attr_d |= 0x80;
        }
        if tech.special_effect {
            attr_d |= 0x40;
        }
        let attributes = [attr_a, attr_b, attr_c, attr_d];

        // Now write it all
        exec_writer
            .write_all(&tech.name_vma_pointer.to_be_bytes())
            .await?;
        exec_writer
            .write_all(&attributes)
            .await?;
        exec_writer.write_all(&tech.field_2.to_le_bytes()).await?;
        exec_writer.write_all(&tech.tp_cost.to_le_bytes()).await?;
        exec_writer
            .write_all(&(tech.target as i32).to_le_bytes())
            .await?;
        exec_writer.write_all(&tech.power.to_le_bytes()).await?;
        exec_writer.write_all(&tech.eusis.to_le_bytes()).await?;
        exec_writer.write_all(&tech.nei.to_le_bytes()).await?;
        exec_writer.write_all(&tech.rudger.to_le_bytes()).await?;
        exec_writer.write_all(&tech.anne.to_le_bytes()).await?;
        exec_writer.write_all(&tech.huey.to_le_bytes()).await?;
        exec_writer.write_all(&tech.amia.to_le_bytes()).await?;
        exec_writer.write_all(&tech.keinz.to_le_bytes()).await?;
        exec_writer.write_all(&tech.silka.to_le_bytes()).await?;
    }
    Ok(())
}

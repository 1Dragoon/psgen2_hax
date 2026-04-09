use crate::{
    events::{
        DialogItem, DialogString, codec::decode_psg2_string, deserialize_dialog_items,
        serialize_dialog_items,
    },
    helpers::{
        deserialize_u8_hex, deserialize_u32_hex, encode_hex, is_default, serialize_u8_hex,
        serialize_u32_hex,
    },
    slpm_patcher::{
        Hexu32, POINTER_OFFSET, RelativePointerInfo, SpellElemental, StringFill, StringMemRegion,
        TECHNIQUE_STRUCT_COUNT, TECHNIQUE_STRUCT_FIELDS, TECHNIQUE_STRUCT_START,
    },
};
use alloc::collections::BTreeMap;
use indexmap::IndexSet;
use log::warn;
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, io};
use strum::{EnumIter, IntoEnumIterator};
use tokio::{
    fs,
    io::{AsyncBufRead, AsyncReadExt, AsyncSeek, AsyncSeekExt, AsyncWriteExt, BufWriter, SeekFrom},
};

#[derive(Serialize, Deserialize)]
#[repr(i32)]
pub enum Targetable {
    #[serde(alias = "sacrificeforall", alias = "sacrifice_for_all")]
    SacrificeForAll = -7,
    #[serde(alias = "sacrificeforone", alias = "sacrifice_for_one")]
    SacrificeForOne = -6,
    #[serde(alias = "allydead", alias = "ally_dead")]
    AllyDead = -5,
    #[serde(alias = "allbutcaster", alias = "all_but_caster")]
    AllButCaster = -4,
    #[serde(alias = "allyall", alias = "ally_all")]
    AllyAll = -3,
    #[serde(alias = "ally")]
    Ally = -2,
    #[serde(alias = "caster")]
    Caster = -1,
    #[serde(alias = "enemy")]
    Enemy = 0,
    #[serde(alias = "enemygroup", alias = "enemy_group")]
    EnemyGroup = 1,
    #[serde(alias = "enemyall", alias = "enemy_all")]
    EnemyAll = 5,
}

impl TryFrom<i32> for Targetable {
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
pub enum Speed {
    #[default]
    #[serde(alias = "medium")]
    Medium = 0x00,
    #[serde(alias = "fast")]
    Fast = 0x10,
    #[serde(alias = "slow")]
    Slow = 0x20,
    #[serde(alias = "speeda", alias = "speed_a")]
    SpeedA = 0x40,
    #[serde(alias = "speedb", alias = "speed_b")]
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
pub enum Usage {
    #[default]
    #[serde(alias = "enemyeffect", alias = "enemy_effect")]
    EnemyEffect = 0x00,
    #[serde(alias = "caster")]
    Caster = 0x01,
    #[serde(alias = "ally")]
    Ally = 0x02,
    #[serde(alias = "enemy")]
    Enemy = 0x04,
    #[serde(alias = "othera", alias = "other_a")]
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

#[repr(u32)]
#[derive(EnumIter, Serialize, Deserialize, Copy, Clone, Eq, PartialEq)]
pub enum SideEffect {
    Seal = 0x0000_0001,
    Paralyze = 0x0000_0002,
    SkillDown = 0x0000_0004,
    RaiseDead = 0x0000_0008,
    UnknownA = 0x0000_0010,
    CureAll = 0x0000_0020,
    UnknownB = 0x0000_0040,
    UnknownC = 0x0000_0080,
    CurePoison = 0x0000_0100,
    TPUp = 0x0000_0200,
    UnknownD = 0x0000_0400,
    Poison = 0x0000_0800,
    AttackUp = 0x0000_1000,
    DefenseUp = 0x0000_2000,
    AgilityUp = 0x0000_4000,
    Heal = 0x0000_8000,
    AttackDown = 0x0001_0000,
    DefenseDown = 0x0002_0000,
    AgilityDown = 0x0004_0000,
    TPDown = 0x0008_0000,
    InstaKill = 0x0010_0000,
    StealHP = 0x0020_0000,
    HeavyDamage = 0x0040_0000,
    NoNumbers = 0x0080_0000,
    UnknownE = 0x0100_0000,
    UnknownF = 0x0200_0000,
    UnknownG = 0x0400_0000,
    UnknownH = 0x0800_0000,
    Sleep = 0x1000_0000,
    UnknownI = 0x2000_0000,
    UnknownJ = 0x4000_0000,
    UnknownK = 0x8000_0000,
}

impl SideEffect {
    fn from_u32(byte: u32) -> Box<[Self]> {
        let mut variants = Vec::with_capacity(8);
        for variant in Self::iter() {
            if byte & variant as u32 == variant as u32 {
                variants.push(variant);
            }
        }
        variants.into_boxed_slice()
    }

    fn to_u32(value: &[Self]) -> u32 {
        let mut byte = 0;
        for variant in value.iter().copied() {
            byte |= variant as u32;
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
    pub name: Vec<DialogItem>,
    #[serde(
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex"
    )]
    pub name_pointer: u32,
    #[serde(
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex"
    )]
    pub name_vma_pointer: u32,
    #[serde(skip)]
    pub text: DialogString,
    #[serde(flatten)]
    pub relative_name_pointer: RelativePointerInfo,
    // #[serde(
    //     default,
    //     serialize_with = "serialize_u32_hex",
    //     deserialize_with = "deserialize_u32_hex",
    //     skip_serializing_if = "is_default"
    // )]
    // attributes: u32,
    #[serde(default, skip_serializing_if = "is_default")]
    pub elemental: Box<[SpellElemental]>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub vulnerable: Box<[VulerableTargets]>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub usage: Box<[Usage]>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub speed: Box<[Speed]>,
    #[serde(
        default,
        serialize_with = "serialize_u8_hex",
        deserialize_with = "deserialize_u8_hex",
        skip_serializing_if = "is_default"
    )]
    pub attr_c: u8,
    #[serde(default, skip_serializing_if = "is_default")]
    pub usable_out_of_combat: bool,
    #[serde(default, skip_serializing_if = "is_default")]
    pub cannot_be_used_in_combat: bool,
    #[serde(default, skip_serializing_if = "is_default")]
    pub side_effect: Box<[SideEffect]>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub tp_cost: u32,
    pub target: Targetable,
    #[serde(default, skip_serializing_if = "is_default")]
    pub power: i32,
    #[serde(default, skip_serializing_if = "is_default")]
    pub eusis: i32,
    #[serde(default, skip_serializing_if = "is_default")]
    pub nei: i32,
    #[serde(default, skip_serializing_if = "is_default")]
    pub rudger: i32,
    #[serde(default, skip_serializing_if = "is_default")]
    pub anne: i32,
    #[serde(default, skip_serializing_if = "is_default")]
    pub huey: i32,
    #[serde(default, skip_serializing_if = "is_default")]
    pub amia: i32,
    #[serde(default, skip_serializing_if = "is_default")]
    pub keinz: i32,
    #[serde(default, skip_serializing_if = "is_default")]
    pub silka: i32,
}

impl StringFill for Technique {
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
        if self.name_vma_pointer != ptr_le {
            warn!(
                "Got {}, expected {}",
                encode_hex(&self.name_vma_pointer.to_le_bytes()),
                encode_hex(&ptr_le.to_le_bytes())
            );
        }
        self.name_vma_pointer = ptr_le;
    }

    fn convert_text(&mut self) {
        self.text = DialogString {
            text: self.name.clone(),
            padding: 0,
        };
    }

    fn is_pointer_aliased(&self) -> bool {
        self.relative_name_pointer.aliased
    }
}

#[repr(u8)]
#[derive(EnumIter, Serialize, Deserialize, Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Debug)]
pub enum VulerableTargets {
    #[serde(alias = "biologic")]
    Biologic = 0x10,
    #[serde(alias = "robotic")]
    Robotic = 0x20,
    #[serde(alias = "notboss", alias = "not_boss")]
    NotBoss = 0x40,
    #[serde(alias = "unknowna", alias = "unknown_a")]
    UnknownA = 0x80,
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
    aliaser: &mut HashSet<u32>,
) -> Result<BTreeMap<Hexu32, Technique>, io::Error> {
    reader
        .seek(SeekFrom::Start(TECHNIQUE_STRUCT_START as u64))
        .await?;
    let mut field_bytes = [0u8; 4];
    let mut techniques = BTreeMap::new();
    let mut relative_pointer_index = IndexSet::with_capacity(TECHNIQUE_STRUCT_COUNT);
    for tech_no in 0..TECHNIQUE_STRUCT_COUNT {
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
        let cannot_be_used_in_combat = attr_d & 0x40 == 0x40;
        // let attributes = u32::from_be_bytes(attributes);
        let side_effect = SideEffect::from_u32(u32::from_be_bytes(fields.pop().unwrap()));
        let tp_cost = u32::from_le_bytes(fields.pop().unwrap());
        let target = Targetable::try_from(i32::from_le_bytes(fields.pop().unwrap())).unwrap();
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
            text: DialogString::default(),
            name_pointer,
            name_vma_pointer: u32::from_be_bytes(pointer_bytes),
            relative_name_pointer: RelativePointerInfo::default(),
            elemental,
            vulnerable,
            speed,
            usage,
            attr_c,
            usable_out_of_combat,
            cannot_be_used_in_combat,
            // attributes,
            side_effect,
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

        techniques.insert(Hexu32(u32::try_from(tech_no).unwrap()), technique);
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
        technique.relative_name_pointer = RelativePointerInfo::new(
            region,
            Hexu32(u32::try_from(index).unwrap()),
            !aliaser.insert(technique.name_pointer),
        );
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
        let attr_a =
            SpellElemental::to_byte(&tech.elemental) | VulerableTargets::to_byte(&tech.vulnerable);
        let attr_b = Speed::to_byte(&tech.speed) | Usage::to_byte(&tech.usage);
        let attr_c = tech.attr_c;
        let mut attr_d = 0;
        if tech.usable_out_of_combat {
            attr_d |= 0x80;
        }
        if tech.cannot_be_used_in_combat {
            attr_d |= 0x40;
        }
        let attributes = [attr_a, attr_b, attr_c, attr_d];
        let side_effect = SideEffect::to_u32(&tech.side_effect).to_be_bytes();

        // Now write it all
        exec_writer
            .write_all(&tech.name_vma_pointer.to_be_bytes())
            .await?;
        exec_writer.write_all(&attributes).await?;
        exec_writer.write_all(&side_effect).await?;
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

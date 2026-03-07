pub mod codec;
pub mod sjis_map;
extern crate alloc;
use crate::{
    events::{
        codec::{DialogMap, OrderedData, OrderedDialog, marshal_events},
        sjis_map::utf8_to_ps2,
    },
    helpers::{
        deserialize_hex, deserialize_indexmap, encode_hex, serialize_hex, serialize_indexmap,
        serialize_rc_empty,
    },
    slpm_patcher::ExecData,
};
use alloc::{
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};
use core::{cell::RefCell, fmt, fmt::Display, mem, str::FromStr};
use indexmap::IndexMap;
use log::{Level, debug, error, log_enabled, trace};
use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{self, DeserializeOwned, Error, Visitor},
};
use std::{
    fs::OpenOptions,
    io::{self, BufReader, BufWriter, Read, Write},
    path::{Path, PathBuf},
};
use unicode_segmentation::UnicodeSegmentation;

const GUESTIMATED_LENGTH: usize = 256;

type Pointer = u32;
type Offset = u32;

#[repr(u8)]
#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all(deserialize = "lowercase"))]
enum ControlCode {
    None,
    Fibrillae, // Also 'c' like color, so it needs special handling
    #[serde(alias = "wait")]
    Push = b'%',
    End = b'\\',
    #[serde(alias = "clear")]
    More = b'?',
    Select = b'*',
    Value = b'$',
    // Newline = b'@',
    Color = b'c',
    Portrait = b'#',
    // Remaining items are specific to goldenboy release
    Armel = b'm',
    Armor = b'`',
    Bandana = b'g',
    Boots = b'd',
    Cake = b'K',
    Cane = b'O',
    Cannon = b'Q',
    Chestplate = b'[',
    Circle = b'}',
    Claw = b'Z',
    Coat = b'a',
    Cross = b'|',
    Crown = b'k',
    Dagger = b'X',
    Espadrilles = b'f',
    Fluid = b'H',
    Gun = b'S',
    Hat = b'h',
    Headgear = b'j',
    Helmet = b'i',
    Important = b'J',
    Knife = b'E',
    Mantle = b'F',
    Mantle2 = b'b',
    Monomate = b'G',
    Moon = b'N',
    Musik = b'v',
    Ocarina = b'I',
    Ribbon = b'l',
    Scale = b'U',
    Scalpel = b'W',
    Shield = b'o',
    Shoes = b'e',
    Shot = b'T',
    Slicer = b'Y',
    Sol = b'L',
    Square = b'~',
    Star = b'M',
    Suit = b'_',
    Sword = b'V',
    Triangle = 0x7F, // <delete>
    Vest = b'^',
    Vulcan = b'R',
    Whip = b'P',
}

impl FromStr for ControlCode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "push" => Ok(Self::Push),
            "end" => Ok(Self::End),
            "more" => Ok(Self::More),
            "select" => Ok(Self::Select),
            "value" => Ok(Self::Value),
            "color" => Ok(Self::Color),
            "portrait" => Ok(Self::Portrait),
            "armel" => Ok(Self::Armel),
            "armor" => Ok(Self::Armor),
            "bandana" => Ok(Self::Bandana),
            "boots" => Ok(Self::Boots),
            "cake" => Ok(Self::Cake),
            "cane" => Ok(Self::Cane),
            "cannon" => Ok(Self::Cannon),
            "chestplate" => Ok(Self::Chestplate),
            "circle" => Ok(Self::Circle),
            "claw" => Ok(Self::Claw),
            "coat" => Ok(Self::Coat),
            "cross" => Ok(Self::Cross),
            "crown" => Ok(Self::Crown),
            "dagger" => Ok(Self::Dagger),
            "espadrilles" => Ok(Self::Espadrilles),
            "fibrillae" => Ok(Self::Fibrillae),
            "fluid" => Ok(Self::Fluid),
            "gun" => Ok(Self::Gun),
            "hat" => Ok(Self::Hat),
            "headgear" => Ok(Self::Headgear),
            "helmet" => Ok(Self::Helmet),
            "important" => Ok(Self::Important),
            "knife" => Ok(Self::Knife),
            "mantle" => Ok(Self::Mantle),
            "mantle2" => Ok(Self::Mantle2),
            "monomate" => Ok(Self::Monomate),
            "moon" => Ok(Self::Moon),
            "musik" => Ok(Self::Musik),
            "ocarina" => Ok(Self::Ocarina),
            "ribbon" => Ok(Self::Ribbon),
            "scale" => Ok(Self::Scale),
            "scalpel" => Ok(Self::Scalpel),
            "shield" => Ok(Self::Shield),
            "shoes" => Ok(Self::Shoes),
            "shot" => Ok(Self::Shot),
            "slicer" => Ok(Self::Slicer),
            "sol" => Ok(Self::Sol),
            "square" => Ok(Self::Square),
            "star" => Ok(Self::Star),
            "suit" => Ok(Self::Suit),
            "sword" => Ok(Self::Sword),
            "triangle" => Ok(Self::Triangle),
            "vest" => Ok(Self::Vest),
            "vulcan" => Ok(Self::Vulcan),
            "whip" => Ok(Self::Whip),
            other => Err(format!("Invalid ControlCode variant: {other}")),
        }
    }
}

impl Display for ControlCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::None => unreachable!(),
            Self::Push => write!(f, "[Push]"),
            Self::End => write!(f, "[End]"),
            Self::More => write!(f, "[More]"),
            Self::Select => write!(f, "[Select]"),
            Self::Value => write!(f, "[Value]"),
            Self::Color => write!(f, "[Color]"),
            Self::Portrait => write!(f, "[Portrait]"),
            Self::Armel => write!(f, "[Armel]"),
            Self::Armor => write!(f, "[Armor]"),
            Self::Bandana => write!(f, "[Bandana]"),
            Self::Boots => write!(f, "[Boots]"),
            Self::Cake => write!(f, "[Cake]"),
            Self::Cane => write!(f, "[Cane]"),
            Self::Cannon => write!(f, "[Cannon]"),
            Self::Chestplate => write!(f, "[Chestplate]"),
            Self::Circle => write!(f, "[Circle]"),
            Self::Claw => write!(f, "[Claw]"),
            Self::Coat => write!(f, "[Coat]"),
            Self::Cross => write!(f, "[Cross]"),
            Self::Crown => write!(f, "[Crown]"),
            Self::Dagger => write!(f, "[Dagger]"),
            Self::Espadrilles => write!(f, "[Espadrilles]"),
            Self::Fibrillae => write!(f, "[Fibrillae]"),
            Self::Fluid => write!(f, "[Fluid]"),
            Self::Gun => write!(f, "[Gun]"),
            Self::Hat => write!(f, "[Hat]"),
            Self::Headgear => write!(f, "[Headgear]"),
            Self::Helmet => write!(f, "[Helmet]"),
            Self::Important => write!(f, "[Important]"),
            Self::Knife => write!(f, "[Knife]"),
            Self::Mantle => write!(f, "[Mantle]"),
            Self::Mantle2 => write!(f, "[Mantle2]"),
            Self::Monomate => write!(f, "[Monomate]"),
            Self::Moon => write!(f, "[Moon]"),
            Self::Musik => write!(f, "[Musik]"),
            Self::Ocarina => write!(f, "[Ocarina]"),
            Self::Ribbon => write!(f, "[Ribbon]"),
            Self::Scale => write!(f, "[Scale]"),
            Self::Scalpel => write!(f, "[Scalpel]"),
            Self::Shield => write!(f, "[Shield]"),
            Self::Shoes => write!(f, "[Shoes]"),
            Self::Shot => write!(f, "[Shot]"),
            Self::Slicer => write!(f, "[Slicer]"),
            Self::Sol => write!(f, "[Sol]"),
            Self::Square => write!(f, "[Square]"),
            Self::Star => write!(f, "[Star]"),
            Self::Suit => write!(f, "[Suit]"),
            Self::Sword => write!(f, "[Sword]"),
            Self::Triangle => write!(f, "[Triangle]"),
            Self::Vest => write!(f, "[Vest]"),
            Self::Vulcan => write!(f, "[Vulcan]"),
            Self::Whip => write!(f, "[Whip]"),
        }
    }
}

impl From<u8> for ControlCode {
    fn from(value: u8) -> Self {
        match value {
            b'%' => Self::Push,
            b'\\' => Self::End,
            b'?' => Self::More,
            b'*' => Self::Select,
            b'$' => Self::Value,
            b'c' => Self::Color, // Also Fibrillae
            b'#' => Self::Portrait,
            0x7F => Self::Triangle,
            b'_' => Self::Suit,
            b'[' => Self::Chestplate,
            b'}' => Self::Circle,
            b'`' => Self::Armor,
            b'^' => Self::Vest,
            b'|' => Self::Cross,
            b'~' => Self::Square,
            b'a' => Self::Coat,
            b'b' => Self::Mantle2,
            b'd' => Self::Boots,
            b'E' => Self::Knife,
            b'e' => Self::Shoes,
            b'f' => Self::Espadrilles,
            b'F' => Self::Mantle,
            b'g' => Self::Bandana,
            b'G' => Self::Monomate,
            b'H' => Self::Fluid,
            b'h' => Self::Hat,
            b'i' => Self::Helmet,
            b'I' => Self::Ocarina,
            b'j' => Self::Headgear,
            b'J' => Self::Important,
            b'K' => Self::Cake,
            b'k' => Self::Crown,
            b'l' => Self::Ribbon,
            b'L' => Self::Sol,
            b'm' => Self::Armel,
            b'M' => Self::Star,
            b'N' => Self::Moon,
            b'O' => Self::Cane,
            b'o' => Self::Shield,
            b'P' => Self::Whip,
            b'Q' => Self::Cannon,
            b'R' => Self::Vulcan,
            b'S' => Self::Gun,
            b'T' => Self::Shot,
            b'U' => Self::Scale,
            b'v' => Self::Musik,
            b'V' => Self::Sword,
            b'W' => Self::Scalpel,
            b'X' => Self::Dagger,
            b'Y' => Self::Slicer,
            b'Z' => Self::Claw,
            _ => Self::None,
        }
    }
}

#[repr(u16)]
#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all(deserialize = "lowercase"))]
enum MTECode {
    None,
    Acid = 0x11a1,
    Amber = 0x129d,
    Animation = 0x1021,
    Antidote = 0x1306,
    Armel = 0x123c,
    Armor = 0x126e,
    Atomizer = 0x12e8,
    BahaBulle = 0x102c,
    Bandanna = 0x127c,
    Beguiling = 0x12ca,
    Black = 0x128d,
    Blazing = 0x11bc,
    Boomerang = 0x11f5,
    Boots = 0x1235,
    Cake = 0x12e3,
    Cane = 0x11e6,
    Cannon = 0x11dc,
    Carbon = 0x121a,
    Card = 0x12e1,
    Ceramic = 0x124e,
    Chain = 0x1265,
    Check = 0x1026,
    Chestplate = 0x1271,
    Chiffon = 0x12f0,
    Claw = 0x11ec,
    Coat = 0x1276,
    Coloring = 0x103b,
    ColoringFinish = 0x1045,
    Commercial = 0x1036,
    Coordinator = 0x1031,
    Covert = 0x12cf,
    Crescent = 0x122b,
    Crown = 0x122f,
    Crystal = 0x121d,
    Dagger = 0x11cb,
    Deco = 0x1065,
    DesignAdvisor = 0x105e,
    Developers = 0x101a,
    Difluid = 0x12bf,
    Dimate = 0x12af,
    EngLoc2015 = 0x106c,
    Espadrilles = 0x1201,
    ExecAdvisor = 0x1077,
    ExecProd = 0x107f,
    Fiberglass = 0x1215,
    Fibrilla = 0x124b,
    Field = 0x1245,
    Flame = 0x125c,
    Fruit = 0x12f3,
    Gale = 0x12a4,
    GameProd = 0x1087,
    GameProgram = 0x1090,
    GameSound = 0x1096,
    Gear = 0x1288,
    GraphicMod = 0x10a9,
    Graphics = 0x109c,
    GraphicsSupport = 0x10b3,
    Guard = 0x128a,
    Gun = 0x11d7,
    Harnisch = 0x1247,
    Hat = 0x127a,
    Heilsam = 0x1294,
    Helmet = 0x1284,
    Hesitant = 0x12d6,
    Hiei = 0x10ba,
    Hrothgar = 0x10bc,
    Ice = 0x11b4,
    Icons = 0x10c2,
    JapanArt = 0x10c5,
    Jewel = 0x1228,
    Key = 0x12ee,
    KeyAnimation = 0x10f4,
    Knife = 0x11c8,
    Kyence = 0x10fb,
    Laconian = 0x1258,
    LargeIndent = 0x093c,
    Laser = 0x1252,
    Leather = 0x1207,
    Lightning = 0x11b6,
    Long = 0x1298,
    Luminous = 0x120e,
    Lyan = 0x10ff,
    MainCharDesign = 0x1101,
    Mantle = 0x1242,
    Marketing = 0x110d,
    Maruera = 0x12c6,
    MidIndent = 0x0928,
    Mirror = 0x1212,
    Mitsuaki = 0x1112,
    Monofluid = 0x12ba,
    Monomate = 0x12aa,
    MontBlanc = 0x12f5,
    Moon = 0x12de,
    NameEnemy = 0x111b,
    Napalm = 0x11a3,
    NaulaStyle = 0x12fa,
    Needle = 0x11f2,
    Nei = 0x1268,
    Ocarina = 0x12c2,
    Original = 0x1126,
    Package = 0x112f,
    Packaging = 0x1134,
    Patcher = 0x1139,
    Plasma = 0x1262,
    Players = 0x1052,
    Producer = 0x113f,
    Production = 0x1143,
    ProgNAssembly = 0x1148,
    PscaveRomhack = 0x10db,
    Publicity = 0x115f,
    PublicRel = 0x1158,
    Pulse = 0x11ad,
    Rainbow = 0x12a6,
    Reco = 0x1013,
    Ribbon = 0x1232,
    Ring = 0x12ec,
    RudolfoCue = 0x1181,
    Saber = 0x11fa,
    SalesSupport = 0x1163,
    Scale = 0x11e9,
    Scalpel = 0x11ce,
    ScenGraphics = 0x1169,
    ScenProd = 0x1171,
    Schneller = 0x1290,
    SegaWow = 0x1001,
    Shield = 0x123f,
    Shoes = 0x129a,
    Shortcake = 0x1301,
    Shot = 0x11d5,
    Shotgun = 0x11ee,
    Silent = 0x126b,
    Silver = 0x1255,
    Sleeve = 0x1239,
    Slicer = 0x11e0,
    SmallIndent = 0x0914,
    Snow = 0x11fd,
    Sol = 0x12da,
    Sonic = 0x11a7,
    SonicTeam = 0x1007,
    SpecialThanks = 0x117a,
    Star = 0x12dc,
    Steel = 0x11c5,
    Sword = 0x11d1,
    Titanium = 0x11b0,
    Tranquil = 0x12a0,
    Translation = 0x1190,
    Traveling = 0x12d2,
    Trimate = 0x12b6,
    Tryphon = 0x1195,
    Vest = 0x1278,
    Vulcan = 0x11d9,
    Wave = 0x11aa,
    Whip = 0x11e3,
    White = 0x1225,
    Windblade = 0x11c0,
    XLIndent = 0x0964,
    Yasunori = 0x1199,
    Zirconium = 0x1220,
}

impl Display for MTECode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::None => unreachable!(),
            Self::Acid => write!(f, "<Acid>"),
            Self::Amber => write!(f, "<Amber>"),
            Self::Animation => write!(f, "<Animation>"),
            Self::Antidote => write!(f, "<Antidote>"),
            Self::Armel => write!(f, "<Armel>"),
            Self::Armor => write!(f, "<Armor>"),
            Self::Atomizer => write!(f, "<Atomizer>"),
            Self::BahaBulle => write!(f, "<BahaBulle>"),
            Self::Bandanna => write!(f, "<Bandanna>"),
            Self::Beguiling => write!(f, "<Beguiling>"),
            Self::Black => write!(f, "<Black>"),
            Self::Blazing => write!(f, "<Blazing>"),
            Self::Boomerang => write!(f, "<Boomerang>"),
            Self::Boots => write!(f, "<Boots>"),
            Self::Cake => write!(f, "<Cake>"),
            Self::Cane => write!(f, "<Cane>"),
            Self::Cannon => write!(f, "<Cannon>"),
            Self::Carbon => write!(f, "<Carbon>"),
            Self::Card => write!(f, "<Card>"),
            Self::Ceramic => write!(f, "<Ceramic>"),
            Self::Chain => write!(f, "<Chain>"),
            Self::Check => write!(f, "<Check>"),
            Self::Chestplate => write!(f, "<Chestplate>"),
            Self::Chiffon => write!(f, "<Chiffon>"),
            Self::Claw => write!(f, "<Claw>"),
            Self::Coat => write!(f, "<Coat>"),
            Self::Coloring => write!(f, "<Coloring>"),
            Self::ColoringFinish => write!(f, "<ColoringFinish>"),
            Self::Commercial => write!(f, "<Commercial>"),
            Self::Coordinator => write!(f, "<Coordinator>"),
            Self::Covert => write!(f, "<Covert>"),
            Self::Crescent => write!(f, "<Crescent>"),
            Self::Crown => write!(f, "<Crown>"),
            Self::Crystal => write!(f, "<Crystal>"),
            Self::Dagger => write!(f, "<Dagger>"),
            Self::Deco => write!(f, "<Deco>"),
            Self::DesignAdvisor => write!(f, "<DesignAdvisor>"),
            Self::Developers => write!(f, "<Developers>"),
            Self::Difluid => write!(f, "<Difluid>"),
            Self::Dimate => write!(f, "<Dimate>"),
            Self::EngLoc2015 => write!(f, "<EngLoc2015>"),
            Self::Espadrilles => write!(f, "<Espadrilles>"),
            Self::ExecAdvisor => write!(f, "<ExecAdvisor>"),
            Self::ExecProd => write!(f, "<ExecProd>"),
            Self::Fiberglass => write!(f, "<Fiberglass>"),
            Self::Fibrilla => write!(f, "<Fibrilla>"),
            Self::Field => write!(f, "<Field>"),
            Self::Flame => write!(f, "<Flame>"),
            Self::Fruit => write!(f, "<Fruit>"),
            Self::Gale => write!(f, "<Gale>"),
            Self::GameProd => write!(f, "<GameProd>"),
            Self::GameProgram => write!(f, "<GameProgram>"),
            Self::GameSound => write!(f, "<GameSound>"),
            Self::Gear => write!(f, "<Gear>"),
            Self::GraphicMod => write!(f, "<GraphicMod>"),
            Self::Graphics => write!(f, "<Graphics>"),
            Self::GraphicsSupport => write!(f, "<GraphicsSupport>"),
            Self::Guard => write!(f, "<Guard>"),
            Self::Gun => write!(f, "<Gun>"),
            Self::Harnisch => write!(f, "<Harnisch>"),
            Self::Hat => write!(f, "<Hat>"),
            Self::Heilsam => write!(f, "<Heilsam>"),
            Self::Helmet => write!(f, "<Helmet>"),
            Self::Hesitant => write!(f, "<Hesitant>"),
            Self::Hiei => write!(f, "<Hiei>"),
            Self::Hrothgar => write!(f, "<Hrothgar>"),
            Self::Ice => write!(f, "<Ice>"),
            Self::Icons => write!(f, "<Icons>"),
            Self::JapanArt => write!(f, "<JapanArt>"),
            Self::Jewel => write!(f, "<Jewel>"),
            Self::Key => write!(f, "<Key>"),
            Self::KeyAnimation => write!(f, "<KeyAnimation>"),
            Self::Knife => write!(f, "<Knife>"),
            Self::Kyence => write!(f, "<Kyence>"),
            Self::Laconian => write!(f, "<Laconian>"),
            Self::LargeIndent => write!(f, "<LargeIndent>"),
            Self::Laser => write!(f, "<Laser>"),
            Self::Leather => write!(f, "<Leather>"),
            Self::Lightning => write!(f, "<Lightning>"),
            Self::Long => write!(f, "<Long>"),
            Self::Luminous => write!(f, "<Luminous>"),
            Self::Lyan => write!(f, "<Lyan>"),
            Self::MainCharDesign => write!(f, "<MainCharDesign>"),
            Self::Mantle => write!(f, "<Mantle>"),
            Self::Marketing => write!(f, "<Marketing>"),
            Self::Maruera => write!(f, "<Maruera>"),
            Self::MidIndent => write!(f, "<MidIndent>"),
            Self::Mirror => write!(f, "<Mirror>"),
            Self::Mitsuaki => write!(f, "<Mitsuaki>"),
            Self::Monofluid => write!(f, "<Monofluid>"),
            Self::Monomate => write!(f, "<Monomate>"),
            Self::MontBlanc => write!(f, "<MontBlanc>"),
            Self::Moon => write!(f, "<Moon>"),
            Self::NameEnemy => write!(f, "<NameEnemy>"),
            Self::Napalm => write!(f, "<Napalm>"),
            Self::NaulaStyle => write!(f, "<NaulaStyle>"),
            Self::Needle => write!(f, "<Needle>"),
            Self::Nei => write!(f, "<Nei>"),
            Self::Ocarina => write!(f, "<Ocarina>"),
            Self::Original => write!(f, "<Original>"),
            Self::Package => write!(f, "<Package>"),
            Self::Packaging => write!(f, "<Packaging>"),
            Self::Patcher => write!(f, "<Patcher>"),
            Self::Plasma => write!(f, "<Plasma>"),
            Self::Players => write!(f, "<Players>"),
            Self::Producer => write!(f, "<Producer>"),
            Self::Production => write!(f, "<Production>"),
            Self::ProgNAssembly => write!(f, "<ProgNAssembly>"),
            Self::PscaveRomhack => write!(f, "<PscaveRomhack>"),
            Self::Publicity => write!(f, "<Publicity>"),
            Self::PublicRel => write!(f, "<PublicRel>"),
            Self::Pulse => write!(f, "<Pulse>"),
            Self::Rainbow => write!(f, "<Rainbow>"),
            Self::Reco => write!(f, "<Reco>"),
            Self::Ribbon => write!(f, "<Ribbon>"),
            Self::Ring => write!(f, "<Ring>"),
            Self::RudolfoCue => write!(f, "<RudolfoCue>"),
            Self::Saber => write!(f, "<Saber>"),
            Self::SalesSupport => write!(f, "<SalesSupport>"),
            Self::Scale => write!(f, "<Scale>"),
            Self::Scalpel => write!(f, "<Scalpel>"),
            Self::ScenGraphics => write!(f, "<ScenGraphics>"),
            Self::ScenProd => write!(f, "<ScenProd>"),
            Self::Schneller => write!(f, "<Schneller>"),
            Self::SegaWow => write!(f, "<SegaWow>"),
            Self::Shield => write!(f, "<Shield>"),
            Self::Shoes => write!(f, "<Shoes>"),
            Self::Shortcake => write!(f, "<Shortcake>"),
            Self::Shot => write!(f, "<Shot>"),
            Self::Shotgun => write!(f, "<Shotgun>"),
            Self::Silent => write!(f, "<Silent>"),
            Self::Silver => write!(f, "<Silver>"),
            Self::Sleeve => write!(f, "<Sleeve>"),
            Self::Slicer => write!(f, "<Slicer>"),
            Self::SmallIndent => write!(f, "<SmallIndent>"),
            Self::Snow => write!(f, "<Snow>"),
            Self::Sol => write!(f, "<Sol>"),
            Self::Sonic => write!(f, "<Sonic>"),
            Self::SonicTeam => write!(f, "<SonicTeam>"),
            Self::SpecialThanks => write!(f, "<SpecialThanks>"),
            Self::Star => write!(f, "<Star>"),
            Self::Steel => write!(f, "<Steel>"),
            Self::Sword => write!(f, "<Sword>"),
            Self::Titanium => write!(f, "<Titanium>"),
            Self::Tranquil => write!(f, "<Tranquil>"),
            Self::Translation => write!(f, "<Translation>"),
            Self::Traveling => write!(f, "<Traveling>"),
            Self::Trimate => write!(f, "<Trimate>"),
            Self::Tryphon => write!(f, "<Tryphon>"),
            Self::Vest => write!(f, "<Vest>"),
            Self::Vulcan => write!(f, "<Vulcan>"),
            Self::Wave => write!(f, "<Wave>"),
            Self::Whip => write!(f, "<Whip>"),
            Self::White => write!(f, "<White>"),
            Self::Windblade => write!(f, "<Windblade>"),
            Self::XLIndent => write!(f, "<XLIndent>"),
            Self::Yasunori => write!(f, "<Yasunori>"),
            Self::Zirconium => write!(f, "<Zirconium>"),
        }
    }
}

impl FromStr for MTECode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "acid" => Ok(Self::Acid),
            "amber" => Ok(Self::Amber),
            "animation" => Ok(Self::Animation),
            "antidote" => Ok(Self::Antidote),
            "armel" => Ok(Self::Armel),
            "armor" => Ok(Self::Armor),
            "atomizer" => Ok(Self::Atomizer),
            "bahabulle" => Ok(Self::BahaBulle),
            "bandanna" => Ok(Self::Bandanna),
            "beguiling" => Ok(Self::Beguiling),
            "black" => Ok(Self::Black),
            "blazing" => Ok(Self::Blazing),
            "boomerang" => Ok(Self::Boomerang),
            "boots" => Ok(Self::Boots),
            "cake" => Ok(Self::Cake),
            "cane" => Ok(Self::Cane),
            "cannon" => Ok(Self::Cannon),
            "carbon" => Ok(Self::Carbon),
            "card" => Ok(Self::Card),
            "ceramic" => Ok(Self::Ceramic),
            "chain" => Ok(Self::Chain),
            "check" => Ok(Self::Check),
            "chestplate" => Ok(Self::Chestplate),
            "chiffon" => Ok(Self::Chiffon),
            "claw" => Ok(Self::Claw),
            "coat" => Ok(Self::Coat),
            "coloring" => Ok(Self::Coloring),
            "coloringfinish" => Ok(Self::ColoringFinish),
            "commercial" => Ok(Self::Commercial),
            "coordinator" => Ok(Self::Coordinator),
            "covert" => Ok(Self::Covert),
            "crescent" => Ok(Self::Crescent),
            "crown" => Ok(Self::Crown),
            "crystal" => Ok(Self::Crystal),
            "dagger" => Ok(Self::Dagger),
            "deco" => Ok(Self::Deco),
            "designadvisor" => Ok(Self::DesignAdvisor),
            "developers" => Ok(Self::Developers),
            "difluid" => Ok(Self::Difluid),
            "dimate" => Ok(Self::Dimate),
            "engloc2015" => Ok(Self::EngLoc2015),
            "espadrilles" => Ok(Self::Espadrilles),
            "execadvisor" => Ok(Self::ExecAdvisor),
            "execprod" => Ok(Self::ExecProd),
            "fiberglass" => Ok(Self::Fiberglass),
            "fibrilla" => Ok(Self::Fibrilla),
            "field" => Ok(Self::Field),
            "flame" => Ok(Self::Flame),
            "fruit" => Ok(Self::Fruit),
            "gale" => Ok(Self::Gale),
            "gameprod" => Ok(Self::GameProd),
            "gameprogram" => Ok(Self::GameProgram),
            "gamesound" => Ok(Self::GameSound),
            "gear" => Ok(Self::Gear),
            "graphicmod" => Ok(Self::GraphicMod),
            "graphics" => Ok(Self::Graphics),
            "graphicssupport" => Ok(Self::GraphicsSupport),
            "guard" => Ok(Self::Guard),
            "gun" => Ok(Self::Gun),
            "harnisch" => Ok(Self::Harnisch),
            "hat" => Ok(Self::Hat),
            "heilsam" => Ok(Self::Heilsam),
            "helmet" => Ok(Self::Helmet),
            "hesitant" => Ok(Self::Hesitant),
            "hiei" => Ok(Self::Hiei),
            "hrothgar" => Ok(Self::Hrothgar),
            "ice" => Ok(Self::Ice),
            "icons" => Ok(Self::Icons),
            "japanart" => Ok(Self::JapanArt),
            "jewel" => Ok(Self::Jewel),
            "key" => Ok(Self::Key),
            "keyanimation" => Ok(Self::KeyAnimation),
            "knife" => Ok(Self::Knife),
            "kyence" => Ok(Self::Kyence),
            "laconian" => Ok(Self::Laconian),
            "largeindent" => Ok(Self::LargeIndent),
            "laser" => Ok(Self::Laser),
            "leather" => Ok(Self::Leather),
            "lightning" => Ok(Self::Lightning),
            "long" => Ok(Self::Long),
            "luminous" => Ok(Self::Luminous),
            "lyan" => Ok(Self::Lyan),
            "mainchardesign" => Ok(Self::MainCharDesign),
            "mantle" => Ok(Self::Mantle),
            "marketing" => Ok(Self::Marketing),
            "maruera" => Ok(Self::Maruera),
            "midindent" => Ok(Self::MidIndent),
            "mirror" => Ok(Self::Mirror),
            "mitsuaki" => Ok(Self::Mitsuaki),
            "monofluid" => Ok(Self::Monofluid),
            "monomate" => Ok(Self::Monomate),
            "montblanc" => Ok(Self::MontBlanc),
            "moon" => Ok(Self::Moon),
            "nameenemy" => Ok(Self::NameEnemy),
            "napalm" => Ok(Self::Napalm),
            "naulastyle" => Ok(Self::NaulaStyle),
            "needle" => Ok(Self::Needle),
            "nei" => Ok(Self::Nei),
            "ocarina" => Ok(Self::Ocarina),
            "original" => Ok(Self::Original),
            "package" => Ok(Self::Package),
            "packaging" => Ok(Self::Packaging),
            "patcher" => Ok(Self::Patcher),
            "plasma" => Ok(Self::Plasma),
            "players" => Ok(Self::Players),
            "producer" => Ok(Self::Producer),
            "production" => Ok(Self::Production),
            "prognassembly" => Ok(Self::ProgNAssembly),
            "pscaveromhack" => Ok(Self::PscaveRomhack),
            "publicity" => Ok(Self::Publicity),
            "publicrel" => Ok(Self::PublicRel),
            "pulse" => Ok(Self::Pulse),
            "rainbow" => Ok(Self::Rainbow),
            "reco" => Ok(Self::Reco),
            "ribbon" => Ok(Self::Ribbon),
            "ring" => Ok(Self::Ring),
            "rudolfocue" => Ok(Self::RudolfoCue),
            "saber" => Ok(Self::Saber),
            "salessupport" => Ok(Self::SalesSupport),
            "scale" => Ok(Self::Scale),
            "scalpel" => Ok(Self::Scalpel),
            "scengraphics" => Ok(Self::ScenGraphics),
            "scenprod" => Ok(Self::ScenProd),
            "schneller" => Ok(Self::Schneller),
            "segawow" => Ok(Self::SegaWow),
            "shield" => Ok(Self::Shield),
            "shoes" => Ok(Self::Shoes),
            "shortcake" => Ok(Self::Shortcake),
            "shot" => Ok(Self::Shot),
            "shotgun" => Ok(Self::Shotgun),
            "silent" => Ok(Self::Silent),
            "silver" => Ok(Self::Silver),
            "sleeve" => Ok(Self::Sleeve),
            "slicer" => Ok(Self::Slicer),
            "smallindent" => Ok(Self::SmallIndent),
            "snow" => Ok(Self::Snow),
            "sol" => Ok(Self::Sol),
            "sonic" => Ok(Self::Sonic),
            "sonicteam" => Ok(Self::SonicTeam),
            "specialthanks" => Ok(Self::SpecialThanks),
            "star" => Ok(Self::Star),
            "steel" => Ok(Self::Steel),
            "sword" => Ok(Self::Sword),
            "titanium" => Ok(Self::Titanium),
            "tranquil" => Ok(Self::Tranquil),
            "translation" => Ok(Self::Translation),
            "traveling" => Ok(Self::Traveling),
            "trimate" => Ok(Self::Trimate),
            "tryphon" => Ok(Self::Tryphon),
            "vest" => Ok(Self::Vest),
            "vulcan" => Ok(Self::Vulcan),
            "wave" => Ok(Self::Wave),
            "whip" => Ok(Self::Whip),
            "white" => Ok(Self::White),
            "windblade" => Ok(Self::Windblade),
            "xlindent" => Ok(Self::XLIndent),
            "yasunori" => Ok(Self::Yasunori),
            "zirconium" => Ok(Self::Zirconium),
            other => Err(format!("Invalid MTECode variant: <{other}>")),
        }
    }
}

impl From<u16> for MTECode {
    fn from(value: u16) -> Self {
        match value {
            0x0914 => Self::SmallIndent,
            0x0928 => Self::MidIndent,
            0x093c => Self::LargeIndent,
            0x0964 => Self::XLIndent,
            0x1001 => Self::SegaWow,
            0x1007 => Self::SonicTeam,
            0x1013 => Self::Reco,
            0x101a => Self::Developers,
            0x1021 => Self::Animation,
            0x1026 => Self::Check,
            0x102c => Self::BahaBulle,
            0x1031 => Self::Coordinator,
            0x1036 => Self::Commercial,
            0x103b => Self::Coloring,
            0x1045 => Self::ColoringFinish,
            0x1052 => Self::Players,
            0x105e => Self::DesignAdvisor,
            0x1065 => Self::Deco,
            0x106c => Self::EngLoc2015,
            0x1077 => Self::ExecAdvisor,
            0x107f => Self::ExecProd,
            0x1087 => Self::GameProd,
            0x1090 => Self::GameProgram,
            0x1096 => Self::GameSound,
            0x109c => Self::Graphics,
            0x10a9 => Self::GraphicMod,
            0x10b3 => Self::GraphicsSupport,
            0x10ba => Self::Hiei,
            0x10bc => Self::Hrothgar,
            0x10c2 => Self::Icons,
            0x10c5 => Self::JapanArt,
            0x10db => Self::PscaveRomhack,
            0x10f4 => Self::KeyAnimation,
            0x10fb => Self::Kyence,
            0x10ff => Self::Lyan,
            0x1101 => Self::MainCharDesign,
            0x110d => Self::Marketing,
            0x1112 => Self::Mitsuaki,
            0x111b => Self::NameEnemy,
            0x1126 => Self::Original,
            0x112f => Self::Package,
            0x1134 => Self::Packaging,
            0x1139 => Self::Patcher,
            0x113f => Self::Producer,
            0x1143 => Self::Production,
            0x1148 => Self::ProgNAssembly,
            0x1158 => Self::PublicRel,
            0x115f => Self::Publicity,
            0x1163 => Self::SalesSupport,
            0x1169 => Self::ScenGraphics,
            0x1171 => Self::ScenProd,
            0x117a => Self::SpecialThanks,
            0x1181 => Self::RudolfoCue,
            0x1190 => Self::Translation,
            0x1195 => Self::Tryphon,
            0x1199 => Self::Yasunori,
            0x11a1 => Self::Acid,
            0x11a3 => Self::Napalm,
            0x11a7 => Self::Sonic,
            0x11aa => Self::Wave,
            0x11ad => Self::Pulse,
            0x11b0 => Self::Titanium,
            0x11b4 => Self::Ice,
            0x11b6 => Self::Lightning,
            0x11bc => Self::Blazing,
            0x11c0 => Self::Windblade,
            0x11c5 => Self::Steel,
            0x11c8 => Self::Knife,
            0x11cb => Self::Dagger,
            0x11ce => Self::Scalpel,
            0x11d1 => Self::Sword,
            0x11d5 => Self::Shot,
            0x11d7 => Self::Gun,
            0x11d9 => Self::Vulcan,
            0x11dc => Self::Cannon,
            0x11e0 => Self::Slicer,
            0x11e3 => Self::Whip,
            0x11e6 => Self::Cane,
            0x11e9 => Self::Scale,
            0x11ec => Self::Claw,
            0x11ee => Self::Shotgun,
            0x11f2 => Self::Needle,
            0x11f5 => Self::Boomerang,
            0x11fa => Self::Saber,
            0x11fd => Self::Snow,
            0x1201 => Self::Espadrilles,
            0x1207 => Self::Leather,
            0x120e => Self::Luminous,
            0x1212 => Self::Mirror,
            0x1215 => Self::Fiberglass,
            0x121a => Self::Carbon,
            0x121d => Self::Crystal,
            0x1220 => Self::Zirconium,
            0x1225 => Self::White,
            0x1228 => Self::Jewel,
            0x122b => Self::Crescent,
            0x122f => Self::Crown,
            0x1232 => Self::Ribbon,
            0x1235 => Self::Boots,
            0x1239 => Self::Sleeve,
            0x123c => Self::Armel,
            0x123f => Self::Shield,
            0x1242 => Self::Mantle,
            0x1245 => Self::Field,
            0x1247 => Self::Harnisch,
            0x124b => Self::Fibrilla,
            0x124e => Self::Ceramic,
            0x1252 => Self::Laser,
            0x1255 => Self::Silver,
            0x1258 => Self::Laconian,
            0x125c => Self::Flame,
            0x1262 => Self::Plasma,
            0x1265 => Self::Chain,
            0x1268 => Self::Nei,
            0x126b => Self::Silent,
            0x126e => Self::Armor,
            0x1271 => Self::Chestplate,
            0x1276 => Self::Coat,
            0x1278 => Self::Vest,
            0x127a => Self::Hat,
            0x127c => Self::Bandanna,
            0x1284 => Self::Helmet,
            0x1288 => Self::Gear,
            0x128a => Self::Guard,
            0x128d => Self::Black,
            0x1290 => Self::Schneller,
            0x1294 => Self::Heilsam,
            0x1298 => Self::Long,
            0x129a => Self::Shoes,
            0x129d => Self::Amber,
            0x12a0 => Self::Tranquil,
            0x12a4 => Self::Gale,
            0x12a6 => Self::Rainbow,
            0x12aa => Self::Monomate,
            0x12af => Self::Dimate,
            0x12b6 => Self::Trimate,
            0x12ba => Self::Monofluid,
            0x12bf => Self::Difluid,
            0x12c2 => Self::Ocarina,
            0x12c6 => Self::Maruera,
            0x12ca => Self::Beguiling,
            0x12cf => Self::Covert,
            0x12d2 => Self::Traveling,
            0x12d6 => Self::Hesitant,
            0x12da => Self::Sol,
            0x12dc => Self::Star,
            0x12de => Self::Moon,
            0x12e1 => Self::Card,
            0x12e3 => Self::Cake,
            0x12e8 => Self::Atomizer,
            0x12ec => Self::Ring,
            0x12ee => Self::Key,
            0x12f0 => Self::Chiffon,
            0x12f3 => Self::Fruit,
            0x12f5 => Self::MontBlanc,
            0x12fa => Self::NaulaStyle,
            0x1301 => Self::Shortcake,
            0x1306 => Self::Antidote,
            _ => Self::None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Portrait(String);

impl Display for Portrait {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[Portrait{}]", self.0)
    }
}

#[repr(u8)]
#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
enum Color {
    Blue = b'1',
    Red = b'2',
    Purple = b'3',
    Green = b'4',
    Cyan = b'5',
    Yellow = b'6',
    White = b'7',
}

impl FromStr for Color {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "blue" => Ok(Self::Blue),
            "red" => Ok(Self::Red),
            "purple" => Ok(Self::Purple),
            "green" => Ok(Self::Green),
            "cyan" => Ok(Self::Cyan),
            "yellow" => Ok(Self::Yellow),
            "white" => Ok(Self::White),
            other => Err(format!("Invalid Color variant: {other}")),
        }
    }
}

impl Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Blue => write!(f, "[Blue]"),
            Self::Red => write!(f, "[Red]"),
            Self::Purple => write!(f, "[Purple]"),
            Self::Green => write!(f, "[Green]"),
            Self::Cyan => write!(f, "[Cyan]"),
            Self::Yellow => write!(f, "[Yellow]"),
            Self::White => write!(f, "[White]"),
        }
    }
}

impl From<u8> for Color {
    fn from(value: u8) -> Self {
        match value {
            b'1' => Self::Blue,
            b'2' => Self::Red,
            b'3' => Self::Purple,
            b'4' => Self::Green,
            b'5' => Self::Cyan,
            b'6' => Self::Yellow,
            b'7' => Self::White,
            other => {
                error!(
                    "Color parse error: Expected numerals 1-7, but got {other:02x}. I'll give you white instead."
                );
                Self::White
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
enum DialogItem {
    Color(Color),
    ControlCode(ControlCode),
    MTECode(MTECode),
    Portrait(Portrait),
    String(String),
}

impl DialogItem {
    fn into_bytes(self) -> Vec<u8> {
        match self {
            Self::ControlCode(ControlCode::Fibrillae) => vec![b'c'],
            Self::ControlCode(cc) => vec![cc as u8],
            Self::Color(color) => vec![ControlCode::Color as u8, color as u8],
            Self::Portrait(portrait) => {
                [vec![ControlCode::Portrait as u8], portrait.0.into_bytes()].concat()
            }
            Self::MTECode(mc) => {
                let mte = (mc as u16).to_be_bytes();
                vec![mte[0], mte[1]]
            }
            Self::String(string) => {
                let mut bytes = Vec::with_capacity(string.len() * 2);
                for g in string.graphemes(true) {
                    bytes.extend(utf8_to_ps2(g).unwrap());
                }
                bytes
            }
        }
    }
}

impl Display for DialogItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ControlCode(control_code) => write!(f, "{control_code}"),
            Self::Color(color) => write!(f, "{color}"),
            Self::MTECode(mc) => write!(f, "{mc}"),
            Self::Portrait(portrait) => write!(f, "{portrait}"),
            Self::String(string) => write!(f, "{string}"),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct DialogString {
    #[serde(default, skip_serializing_if = "is_false")]
    padded: bool,
    #[serde(
        deserialize_with = "deserialize_dialog_items",
        serialize_with = "serialize_dialog_items"
    )]
    text: Vec<DialogItem>,
}

impl Display for DialogString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for item in &self.text {
            write!(f, "{item}")?;
        }
        Ok(())
    }
}

impl DialogString {
    pub fn into_bytes(self, est_offset: Option<usize>) -> Vec<u8> {
        // Pass a value into offset to calculate padding where needed
        // Or pass None to ignore padding, even if specified
        let mut string_bytes = Vec::with_capacity(255);
        let Self { text, padded } = self;
        for item in text {
            string_bytes.extend(item.into_bytes());
        }
        if let Some(eo) = est_offset
            && padded
        {
            while !(eo + string_bytes.len()).is_multiple_of(4) {
                string_bytes.push(0);
            }
        }
        string_bytes.shrink_to_fit();
        string_bytes
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum Data {
    Cop(u8, u8, u8, Pointer),
    Cop2(u8, u8, u32, Pointer),
    J(u8, Pointer),
    Jal(u8, u8, u8, u32, u32, Pointer),
    Multi(u8, Pointer, Vec<u32>),
    // Just a solo pointer. Carries no opcode.
    Ptr(Pointer),
    Ret,
    #[serde(serialize_with = "serialize_rc_empty")]
    String(Rc<RefCell<Vec<u8>>>),
    TxtPtr(Pointer),
    #[serde(serialize_with = "serialize_hex", deserialize_with = "deserialize_hex")]
    Unmanaged(Vec<u8>),
}

impl Data {
    const fn get_pointer(&self) -> Option<Pointer> {
        match self {
            Self::J(_, pointer)
            | Self::Jal(_, _, _, _, _, pointer)
            | Self::Multi(_, pointer, _)
            | Self::TxtPtr(pointer)
            | Self::Cop(_, _, _, pointer)
            | Self::Cop2(_, _, _, pointer)
            // For those ops with two pointers, this takes the place of that other pointer
            | Self::Ptr(pointer) => Some(*pointer),
            Self::Ret | Self::String(_) | Self::Unmanaged(_) => None,
        }
    }

    fn into_bytes(self) -> Vec<u8> {
        match self {
            Self::Ret => vec![0x0a, 0x00, 0x00, 0x00],
            Self::J(op, pointer) => [[op, 0x00, 0x00, 0x00], pointer.to_le_bytes()].concat(),
            Self::Jal(op, c, d, f1, f2, pointer) => {
                let op_bytes = [op, 0x00, c, d];
                let mut bytes = Vec::with_capacity(self.len());
                bytes.extend(op_bytes);
                bytes.extend(f1.to_le_bytes());
                bytes.extend(f2.to_le_bytes());
                bytes.extend(pointer.to_le_bytes());
                bytes
            }
            Self::Multi(op, pointer, items) => {
                let mut bytes = Vec::with_capacity(4 + (items.len() * 4) + size_of_val(&pointer));
                bytes.extend([op, 0x00, u8::try_from(items.len()).unwrap(), 0x00]);
                bytes.extend(pointer.to_le_bytes());
                for item in items {
                    bytes.extend(item.to_le_bytes());
                }
                bytes
            }
            Self::TxtPtr(pointer) => {
                let op_bytes = [0x12, 0x00, 0x00, 0x00];
                let mut bytes = Vec::with_capacity(self.len());
                bytes.extend(op_bytes);
                bytes.extend(pointer.to_le_bytes());
                bytes
            }
            Self::String(string) => string.borrow().clone(),
            Self::Cop(op, c, d, pointer) => [[op, 0x00, c, d], pointer.to_le_bytes()].concat(),
            Self::Cop2(op, c, field, pointer) => [
                [op, 0x00, c, 0x00],
                field.to_le_bytes(),
                pointer.to_le_bytes(),
            ]
            .concat(),
            Self::Ptr(pointer) => pointer.to_le_bytes().to_vec(),
            Self::Unmanaged(bytes) => bytes,
        }
    }

    fn len(&self) -> usize {
        match self {
            Self::Ret => 4,
            Self::J(_op, pointer) => 4 + size_of_val(pointer),
            Self::Jal(_op, _c, _d, field_1, field_2, pointer) => {
                4 + size_of_val(field_1) + size_of_val(field_2) + size_of_val(pointer)
            }
            Self::Multi(_op, pointer, values) => 4 + (values.len() * 4) + size_of_val(pointer),
            Self::TxtPtr(pointer) => 4 + size_of_val(pointer),
            Self::String(string) => string.borrow().len(),
            Self::Cop(_op, _c, _d, pointer) => 4 + size_of_val(pointer),
            Self::Cop2(_op, _c, field, pointer) => 4 + size_of_val(field) + size_of_val(pointer),
            Self::Ptr(pointer) => size_of_val(pointer),
            Self::Unmanaged(bytes) => bytes.len(),
        }
    }

    const fn set_pointer_symbol(&mut self, symbol: Pointer) {
        // Replace the actual pointer value with a symbol that won't change no matter how the pointers are moved or mutated
        // This allows dialog files to be reused seamlessly across different translations without the user having to muck with offsets manually.
        match self {
            Self::J(_, pointer)
            | Self::Jal(_, _, _, _, _, pointer)
            | Self::Multi(_, pointer, _)
            | Self::TxtPtr(pointer)
            | Self::Cop(_, _, _, pointer)
            | Self::Cop2(_, _, _, pointer)
            | Self::Ptr(pointer) => *pointer = symbol,
            Self::Ret | Self::String(_) | Self::Unmanaged(_) => {}
        }
    }
}

#[expect(clippy::panic_in_result_fn, reason = "condition won't fail")]
impl Display for Data {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ret => {
                write!(f, "Return")?;
            }
            Self::J(op, pointer) => {
                write!(f, "{} {op:02x} -> ({pointer:04x})", op_to_str(*op))?;
            }
            Self::Jal(op, c, d, field_1, field_2, pointer) => {
                write!(
                    f,
                    "{} {op:02x}{c:02x}{d:02x} : {field_1:04x} {field_2:04x} -> ({pointer:04x})",
                    op_to_str(*op)
                )?;
            }
            Self::Multi(op, pointer, values) => {
                write!(f, "{} {op:02x} values: ", op_to_str(*op))?;
                for (i, value) in values.iter().enumerate() {
                    write!(f, "{} {value:04x} ", i + 1)?;
                }
                write!(f, "-> ({pointer:04x})")?;
            }
            Self::TxtPtr(pointer) => {
                write!(f, "Text: -> ({pointer:04x})")?;
            }
            Self::String(string) => {
                write!(f, "String data: (size: {:4})", string.borrow().len())?;
            }
            Self::Cop(op, c, d, pointer) => {
                write!(f, "{} {c:02x}{d:02x} -> ({pointer:04x})", op_to_str(*op))?;
            }
            Self::Cop2(op, c, field, pointer) => {
                write!(
                    f,
                    "{} {c:02x}-{field:04x} -> ({pointer:04x})",
                    op_to_str(*op)
                )?;
            }
            Self::Ptr(pointer) => {
                write!(f, "Ptr -> ({pointer:04x})")?;
            }
            Self::Unmanaged(bytes) => {
                writeln!(f, "Unmanaged data: (size: {:4})", bytes.len())?;
                let mut previous_chunk = [0u8; 4];
                let mut previous_chunk_int = 0;
                for chunk in bytes.chunks(32) {
                    write!(f, "   ")?;
                    for c in chunk.chunks(4) {
                        write!(f, " {}", encode_hex(c))?;
                        #[expect(clippy::indexing_slicing, reason = "the checks are sufficient")]
                        let chunk_bytes = {
                            assert!(c.len() > 3, "while checking pointer in display output");
                            [c[0], c[1], c[2], c[3]]
                        };
                        let chunk_int = u32::from_le_bytes(chunk_bytes);
                        if chunk_int < 0xffff
                            && chunk_int % 4 == 0
                            && chunk_int > 0xff
                            && previous_chunk_int > 0
                        {
                            write!(
                                f,
                                "\nPossible pointer: {} {} [{chunk_int:04x}]\n   ",
                                encode_hex(&previous_chunk),
                                encode_hex(&chunk_bytes)
                            )?;
                        }
                        previous_chunk = chunk_bytes;
                        previous_chunk_int = chunk_int;
                    }
                    writeln!(f)?;
                }
            }
        }
        Ok(())
    }
}

enum UnmanagedDataState {
    Building,
    Idle,
}

struct UmanagedData {
    data: Vec<u8>,
    offset: Offset,
    state: UnmanagedDataState,
}

impl UmanagedData {
    fn finish(&mut self, eof: Offset, data_items: &mut DataItems) {
        match self.state {
            UnmanagedDataState::Building => {
                let mut data_a = Vec::with_capacity(GUESTIMATED_LENGTH);
                let data_b = &mut self.data;
                mem::swap(&mut data_a, data_b);
                data_items.insert(self.offset, eof, Data::Unmanaged(data_a));
                self.state = UnmanagedDataState::Idle;
            }
            UnmanagedDataState::Idle => {
                // Nothing to do
            }
        }
    }

    fn new() -> Self {
        Self {
            state: UnmanagedDataState::Idle,
            offset: 0,
            data: Vec::with_capacity(GUESTIMATED_LENGTH),
        }
    }

    fn update(&mut self, current_offset: Offset, data: [u8; 4]) {
        match self.state {
            UnmanagedDataState::Building => {
                self.data.extend(&data);
            }
            UnmanagedDataState::Idle => {
                self.state = UnmanagedDataState::Building;
                self.offset = current_offset;
                self.data.extend(&data);
            }
        }
    }
}

struct DataItems {
    data_items: BTreeMap<Offset, Data>,
    pointer_tracker: BTreeSet<Offset>,
}

impl DataItems {
    fn insert(&mut self, data_offset: Offset, eof: Offset, mut data: Data) {
        assert!(
            !self.data_items.contains_key(&data_offset),
            "Tried to overwrite an old offset! They can only be mutated!"
        );
        if let Some(pointer) = data.get_pointer() {
            // println!("[{data_offset:04x}] ({pointer:04x}) {data}");
            if pointer > eof {
                data = Data::Unmanaged(data.into_bytes());
            } else {
                self.pointer_tracker.insert(pointer);
                if pointer < data_offset && !self.data_items.contains_key(&pointer) {
                    self.lookback(pointer);
                }
            }
        }
        self.data_items.insert(data_offset, data);
    }

    fn into_ordered_data(self, mut dialog_data: DialogMap) -> (OrderedData, OrderedDialog) {
        let mut ordered_data = IndexMap::with_capacity(self.data_items.len());
        let mut ordered_dialog = IndexMap::with_capacity(dialog_data.len());
        let mut data_items_iter = self.data_items.into_iter();
        let mut current_section = 0;
        let mut pointer_symbols = BTreeMap::new();
        pointer_symbols.insert(0, 0);

        // Convert all pointers into pointer symbols, and layout data along with the most recent pointer symbol.
        for (offset, mut data) in data_items_iter.by_ref() {
            // If the current data item has a pointer, convert it into a symbol
            if let Some(data_pointer) = data.get_pointer() {
                let mut symbol = Pointer::try_from(pointer_symbols.len()).unwrap();
                symbol = *pointer_symbols.entry(data_pointer).or_insert(symbol);
                data.set_pointer_symbol(symbol);
            }
            // If the current offset is referred to by a pointer, create a symbol for it
            if let Some(pointer) = self.pointer_tracker.get(&offset) {
                let symbol = Pointer::try_from(pointer_symbols.len()).unwrap();
                pointer_symbols.entry(*pointer).or_insert(symbol);
            }
            if let Some(symbol) = pointer_symbols.get(&offset) {
                current_section = *symbol;
            }
            if let Some(dialog_item) = dialog_data.remove(&offset) {
                let mut symbol = Pointer::try_from(pointer_symbols.len()).unwrap();
                symbol = *pointer_symbols.entry(offset).or_insert(symbol);
                ordered_dialog.insert(symbol, dialog_item);
            }
            let data_set = ordered_data
                .entry(current_section)
                .or_insert(Vec::with_capacity(16));
            data_set.push(data);
        }

        if log_enabled!(Level::Debug) {
            for (symbol, data_items) in &ordered_data {
                for item in data_items {
                    debug!("Symbol: {symbol:04x}, Data: {item}");
                }
            }
        }

        (ordered_data, ordered_dialog)
    }

    fn lookback(&mut self, pointer: Offset) {
        let mut next_item = None;
        if let Some((offset, data)) = self.data_items.range_mut(..pointer).next_back() {
            trace!("Lookback to {pointer:04x} => {data}");
            if let Data::Unmanaged(bytes) = data {
                next_item = Some(Data::Unmanaged(
                    bytes.split_off((pointer - offset) as usize),
                ));
            }
        }
        if let Some(data) = next_item {
            self.data_items.insert(pointer, data);
        }
    }

    const fn new() -> Self {
        Self {
            data_items: BTreeMap::new(),
            pointer_tracker: BTreeSet::new(),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct IndexMapWrapper<T: Serialize + DeserializeOwned>(
    #[serde(
        deserialize_with = "deserialize_indexmap",
        serialize_with = "serialize_indexmap"
    )]
    pub(crate) IndexMap<u32, T>,
);

#[derive(Serialize, Deserialize)]
enum BytesOrPointer {
    Bytes(Vec<u8>),
    PadBytes,
    Pointer(Pointer),
}

#[inline]
fn op_to_str(op: u8) -> &'static str {
    match op {
        0x0b => "J",
        0x38 => "XORI",
        0x1e => "BGTZ",
        0x17 => "BNE",
        0x41 => "COP",
        0x4a => "COP2",
        0x33 => "ANDI",
        0x00 => "SLL",
        0x24 | 0x25 => "ADDIU",
        0x0c | 0x0f => "JAL",
        0x10 => "BEQ",
        _ => {
            error!("Bad opcode {op:02x}");
            "Bad Opcode!"
        }
    }
}

#[expect(clippy::indexing_slicing, reason = "the checks are sufficient")]
fn parse_dialog(input: &str) -> Result<Vec<DialogItem>, String> {
    let mut out = Vec::new();

    // One entry per user-perceived character (grapheme cluster)
    let mut graphemes: Vec<&str> = input.graphemes(true).collect();
    // Remove the very last newline character, if present
    if graphemes.last().is_some_and(|g| *g == "\n") {
        graphemes.pop();
    }
    let mut i = 0;

    while i < graphemes.len() {
        if graphemes[i] == "[" {
            // Offset just after the opening square bracket
            let start = i + 1;
            // Offset just before the closing square bracket
            let closing = graphemes[start..]
                .iter()
                .position(|g| *g == "]")
                .map(|p| start + p)
                .ok_or_else(|| "Unclosed '['".to_owned())?;

            // Concatonate the tag contents into a string
            let mut inner = graphemes[start..closing].concat();
            // Throw an error if the tag is empty
            if inner.is_empty() {
                return Err("Empty [] block".to_owned());
            }
            inner = inner.to_lowercase();

            // Parse the tag name into an actual tag object
            if let Some(num) = inner.strip_prefix("portrait") {
                out.push(DialogItem::Portrait(Portrait(num.trim().to_owned())));
            } else {
                // Try Color first
                if let Ok(col) = Color::from_str(&inner) {
                    out.push(DialogItem::Color(col));
                }
                // Then ControlCode
                else if let Ok(cc) = ControlCode::from_str(&inner) {
                    out.push(DialogItem::ControlCode(cc));
                } else {
                    return Err(format!("Unknown square bracket tag: {inner}"));
                }
            }

            // Skip past ']'
            i = closing + 1;
        } else if graphemes[i] == "<" {
            // Offset just after the opening angle bracket
            let start = i + 1;
            // Offset just before the closing angle bracket
            let closing = graphemes[start..]
                .iter()
                .position(|g| *g == ">")
                .map(|p| start + p)
                .ok_or_else(|| "Unclosed '<'".to_owned())?;
            // Concatonate the tag contents into a string
            let mut inner = graphemes[start..closing].concat();
            // Throw an error if the tag is empty
            if inner.is_empty() {
                return Err("Empty <> block".to_owned());
            }
            inner = inner.to_lowercase();
            if let Ok(mc) = MTECode::from_str(&inner) {
                out.push(DialogItem::MTECode(mc));
            } else {
                return Err(format!("Unknown angle bracket tag: {inner}"));
            }
            // Skip past '>'
            i = closing + 1;
        } else {
            // Read the text until we get to the next tag opener
            let start = i;
            while i < graphemes.len() && !["[", "<"].contains(&graphemes[i]) {
                i += 1;
            }
            // If we get any text at all, add it
            if i > start {
                let text: String = graphemes[start..i].iter().copied().collect();
                out.push(DialogItem::String(text));
            }
        }
    }

    Ok(out)
}

pub fn save_dialog_strings(
    path: &PathBuf,
    dialog: &IndexMapWrapper<DialogString>,
) -> Result<(), io::Error> {
    let strings = toml::to_string(&dialog).unwrap();

    let file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(path)?;
    let mut bw = BufWriter::new(file);
    bw.write_all(strings.as_bytes())?;
    Ok(())
}

// pub(crate) fn save_event_data(
//     path: &PathBuf,
//     events: &IndexMapWrapper<Vec<Data>>,
// ) -> Result<(), io::Error> {
//     let strings = serde_json::to_string(&events).unwrap();

//     let f = OpenOptions::new()
//         .create(true)
//         .truncate(true)
//         .write(true)
//         .open(path)?;
//     let mut f = BufWriter::new(f);
//     f.write_all(strings.as_bytes())?;
//     Ok(())
// }

// pub(crate) fn load_event_data(path: &PathBuf) -> Result<OrderedData, io::Error> {
//     let f = OpenOptions::new().read(true).open(path)?;
//     let mut string = String::with_capacity(f.metadata().unwrap().len() as usize);
//     let mut f = BufReader::new(f);
//     f.read_to_string(&mut string)?;
//     Ok(serde_json::from_str::<IndexMapWrapper<Vec<Data>>>(&string)?.0)
// }

pub fn rebuild_event<P: AsRef<Path>>(
    data: &[u8],
    file_name: &str,
    dialog_file_path: P,
) -> Result<Vec<u8>, io::Error> {
    let ordered_data = serde_json::from_slice::<IndexMapWrapper<Vec<Data>>>(data)?.0;
    let dialog_items = if dialog_file_path.as_ref().exists() {
        Some(load_dialog_strings(dialog_file_path.as_ref())?)
    } else {
        None
    };

    let event_data = marshal_events(
        // &data,
        ordered_data,
        dialog_items,
        file_name,
    );
    Ok(event_data)
}

pub fn load_dialog_strings<P: AsRef<Path>>(path: P) -> Result<OrderedDialog, io::Error> {
    let file = OpenOptions::new().read(true).open(path)?;
    let mut string =
        String::with_capacity(usize::try_from(file.metadata().unwrap().len()).unwrap());
    let mut br = BufReader::new(file);
    br.read_to_string(&mut string)?;
    Ok(toml::from_str::<IndexMapWrapper<DialogString>>(&string)
        .unwrap()
        .0)
}

pub fn load_exec_patch<P: AsRef<Path>>(path: P) -> Result<ExecData, io::Error> {
    let file = OpenOptions::new().read(true).open(path)?;
    let mut string =
        String::with_capacity(usize::try_from(file.metadata().unwrap().len()).unwrap());
    let mut br = BufReader::new(file);
    br.read_to_string(&mut string)?;
    Ok(serde_json::from_str::<ExecData>(&string).unwrap())
}

#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "satisfies trait requirement"
)]
fn is_false(val: &bool) -> bool {
    !val
}

fn serialize_dialog_items<S>(x: &Vec<DialogItem>, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let mut string = String::with_capacity(120);
    for item in x {
        string.push_str(item.to_string().as_str());
    }
    string.push('\n');
    s.serialize_str(string.as_str())
}

fn deserialize_dialog_items<'de, D>(deserializer: D) -> Result<Vec<DialogItem>, D::Error>
where
    D: Deserializer<'de>,
{
    struct DialogVisitor;

    impl Visitor<'_> for DialogVisitor {
        type Value = Vec<DialogItem>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str(
                "a string mixed with any or all of [Tags], <Tags>, Japanese and English UTF8 text",
            )
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: Error,
        {
            parse_dialog(v).map_err(de::Error::custom)
        }
    }

    deserializer.deserialize_str(DialogVisitor)
}

// for i in 0..(reconstituted_data.len().min(data.len())) {
//     let j = i.saturating_sub(8);
//     if reconstituted_data[i] != data[i] {
//         panic!(
//             "Event {stem_name} Beginning at {j:04x}\nGot: \n{}\nExpected:\n{}. Output is likely corrupted.",
//             encode_hex(&reconstituted_data[j..i + 8]),
//             encode_hex(&data[j..i + 8])
//         )
//     }
// }
// if reconstituted_data.len() != data.len() {
//     warn!(
//         "Event {stem_name} Reconstituted data is {} bytes, original is {} bytes\nGot: \n{}\nExpected:\n{}. Output is likely corrupted.",
//         reconstituted_data.len(),
//         data.len(),
//         encode_hex(&reconstituted_data[reconstituted_data.len() - 8..]),
//         encode_hex(&data[data.len() - 8..])
//     )
// }

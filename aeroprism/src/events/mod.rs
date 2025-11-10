pub mod codec;
pub mod sjis_map;
extern crate alloc;
use crate::{
    events::{
        codec::{DialogMap, OrderedData, OrderedDialog, marshal_events},
        sjis_map::utf8_to_ps2,
    },
    helpers::{decode_hex, encode_hex},
};
use alloc::{
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};
use core::{cell::RefCell, fmt, fmt::Display, str::FromStr, mem};
use indexmap::IndexMap;
use log::{debug, error, trace};
use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{self, DeserializeOwned, Error, Visitor},
    ser::SerializeSeq,
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

fn serialize_hex<S>(x: &[u8], s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    s.serialize_str(&encode_hex(x))
}

fn deserialize_hex<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
where
    D: Deserializer<'de>,
{
    struct HexVisitor;

    impl Visitor<'_> for HexVisitor {
        type Value = Vec<u8>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("hexadecimal string to bytes")
        }

        fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
        where
            E: Error,
        {
            decode_hex(s).map_err(de::Error::custom)
        }
    }

    deserializer.deserialize_str(HexVisitor)
}

fn serialize_rc_empty<S>(_: &Rc<RefCell<Vec<u8>>>, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    s.serialize_seq(Some(0))?.end()
}

fn serialize_u32_hex<S>(x: &u32, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    s.serialize_str(format!("{x:04x}").as_str())
}

fn deserialize_u32_hex<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: Deserializer<'de>,
{
    struct U32visitor;

    impl Visitor<'_> for U32visitor {
        type Value = u32;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a two byte hex string")
        }

        fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
        where
            E: Error,
        {
            let mut bytes = decode_hex(s).map_err(de::Error::custom)?;
            let fourth = bytes.pop().unwrap_or_default();
            let third = bytes.pop().unwrap_or_default();
            let second = bytes.pop().unwrap_or_default();
            let first = bytes.pop().unwrap_or_default();
            Ok(u32::from_be_bytes([first, second, third, fourth]))
        }
    }

    deserializer.deserialize_str(U32visitor)
}

#[repr(u8)]
#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all(deserialize = "lowercase"))]
enum ControlCode {
    None,
    #[serde(alias = "wait")]
    Push = b'%',
    End = b'\\',
    #[serde(alias = "clear")]
    More = b'?',
    Select = b'*',
    Value = b'$',
    Important = b'J', // Goldenboy release specific
    Musik = b'v',     // Goldenboy release specific
    Sword = b'V',     // Goldenboy release specific
    Cross = b'|',     // Goldenboy release specific
    Triangle = 0x7F,  // <delete> // Goldenboy release specific
    Square = b'~',    // Goldenboy release specific
    Circle = b'}',    // Goldenboy release specific
    Claw = b'Z',      // Goldenboy release specific
    Star = b'M',      // Goldenboy release specific
    Sol = b'L',       // Goldenboy release specific
    Crown = b'k',     // Goldenboy release specific
    Helmet = b'i',    // Goldenboy release specific
    Fluid = b'H',     // Goldenboy release specific
    Moon = b'N',      // Goldenboy release specific
    Hat = b'h',       // Goldenboy release specific
    // Newline = b'@',
    Color = b'c',
    Portrait = b'#',
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
            "important" => Ok(Self::Important),
            "musik" => Ok(Self::Musik),
            "sword" => Ok(Self::Sword),
            "cross" => Ok(Self::Cross),
            "triangle" => Ok(Self::Triangle),
            "square" => Ok(Self::Square),
            "circle" => Ok(Self::Circle),
            "claw" => Ok(Self::Claw),
            "star" => Ok(Self::Star),
            "sol" => Ok(Self::Sol),
            "crown" => Ok(Self::Crown),
            "helmet" => Ok(Self::Helmet),
            "fluid" => Ok(Self::Fluid),
            "moon" => Ok(Self::Moon),
            "hat" => Ok(Self::Hat),
            "color" => Ok(Self::Color),
            "portrait" => Ok(Self::Portrait),
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
            Self::Important => write!(f, "[Important]"),
            Self::Musik => write!(f, "[Musik]"),
            Self::Sword => write!(f, "[Sword]"),
            Self::Cross => write!(f, "[Cross]"),
            Self::Triangle => write!(f, "[Triangle]"),
            Self::Square => write!(f, "[Square]"),
            Self::Circle => write!(f, "[Circle]"),
            Self::Claw => write!(f, "[Claw]"),
            Self::Star => write!(f, "[Star]"),
            Self::Sol => write!(f, "[Sol]"),
            Self::Crown => write!(f, "[Crown]"),
            Self::Helmet => write!(f, "[Helmet]"),
            Self::Fluid => write!(f, "[Fluid]"),
            Self::Moon => write!(f, "[Moon]"),
            Self::Hat => write!(f, "[Hat]"),
            Self::Color => write!(f, "[Color]"),
            Self::Portrait => write!(f, "[Portrait]"),
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
            b'J' => Self::Important,
            b'v' => Self::Musik,
            b'V' => Self::Sword,
            b'|' => Self::Cross,
            0x7F => Self::Triangle,
            b'~' => Self::Square,
            b'}' => Self::Circle,
            b'Z' => Self::Claw,
            b'M' => Self::Star,
            b'L' => Self::Sol,
            b'k' => Self::Crown,
            b'i' => Self::Helmet,
            b'H' => Self::Fluid,
            b'N' => Self::Moon,
            b'h' => Self::Hat,
            b'c' => Self::Color,
            b'#' => Self::Portrait,
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
    Portrait(Portrait),
    String(String),
}

impl DialogItem {
    fn into_bytes(self) -> Vec<u8> {
        match self {
            Self::ControlCode(cc) => vec![cc as u8],
            Self::Color(color) => vec![ControlCode::Color as u8, color as u8],
            Self::Portrait(portrait) => {
                [vec![ControlCode::Portrait as u8], portrait.0.into_bytes()].concat()
            }
            Self::String(string) => {
                let mut bytes = Vec::with_capacity(string.len() * 2);
                for g in string.graphemes(true) {
                    bytes.extend(utf8_to_ps2(g));
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
            Self::Portrait(portrait) => write!(f, "{portrait}"),
            Self::String(string) => write!(f, "{string}"),
        }
    }
}

fn is_false(val: &bool) -> bool {
    !val
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct DialogString {
    #[serde(
        deserialize_with = "deserialize_dialog_items",
        serialize_with = "serialize_dialog_items"
    )]
    text: Vec<DialogItem>,
    #[serde(default, skip_serializing_if = "is_false")]
    padded: bool,
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
            formatter.write_str("a string mixed with [Tags] and Japanese or English UTF8 text")
        }

        fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
        where
            E: Error,
        {
            parse_dialog(s).map_err(de::Error::custom)
        }
    }

    deserializer.deserialize_str(DialogVisitor)
}

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
                    return Err(format!("Unknown tag: {inner}"));
                }
            }

            // Skip past ']'
            i = closing + 1;
        } else {
            // Read the text until we get to the next tag opener
            let start = i;
            while i < graphemes.len() && graphemes[i] != "[" {
                i += 1;
            }

            let text: String = graphemes[start..i].iter().copied().collect();

            // Don't push this text item unless it actually contains text
            if !text.is_empty() {
                out.push(DialogItem::String(text));
            }
        }
    }

    Ok(out)
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum Data {
    Ret,
    J(u8, Pointer),
    Jal(u8, u8, u8, u32, u32, Pointer),
    Multi(u8, Pointer, Vec<u32>),
    TxtPtr(Pointer),
    #[serde(serialize_with = "serialize_rc_empty")]
    String(Rc<RefCell<Vec<u8>>>),
    Cop(u8, u8, u8, Pointer),
    Cop2(u8, u8, u32, Pointer),
    // Just a solo pointer. Carries no opcode.
    Ptr(Pointer),
    #[serde(serialize_with = "serialize_hex", deserialize_with = "deserialize_hex")]
    Unmanaged(Vec<u8>),
}

impl Data {
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
                bytes.extend([op, 0x00, items.len() as u8, 0x00]);
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

    const fn get_pointer(&self) -> Option<Pointer> {
        match self {
            Self::Ret => None,
            Self::J(_, pointer) => Some(*pointer),
            Self::Jal(_, _, _, _, _, pointer) => Some(*pointer),
            Self::Multi(_, pointer, _) => Some(*pointer),
            Self::TxtPtr(pointer) => Some(*pointer),
            Self::String(_) => None,
            Self::Cop(_, _, _, pointer) => Some(*pointer),
            Self::Cop2(_, _, _, pointer) => Some(*pointer),
            // For those ops with two pointers, this takes the place of that other pointer
            Self::Ptr(pointer) => Some(*pointer),
            Self::Unmanaged(_) => None,
        }
    }

    const fn set_pointer_symbol(&mut self, symbol: Pointer) {
        // Replace the actual pointer value with a symbol that won't change no matter how the pointers are moved or mutated
        // This allows dialog files to be reused seamlessly across different translations without the user having to muck with offsets manually.
        match self {
            Self::Ret => {}
            Self::J(_, pointer) => *pointer = symbol,
            Self::Jal(_, _, _, _, _, pointer) => *pointer = symbol,
            Self::Multi(_, pointer, _) => *pointer = symbol,
            Self::TxtPtr(pointer) => *pointer = symbol,
            Self::String(_) => {}
            Self::Cop(_, _, _, pointer) => *pointer = symbol,
            Self::Cop2(_, _, _, pointer) => *pointer = symbol,
            Self::Ptr(pointer) => *pointer = symbol,
            Self::Unmanaged(_) => {}
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
    state: UnmanagedDataState,
    offset: Offset,
    data: Vec<u8>,
}

impl UmanagedData {
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
}

struct DataItems {
    data_items: BTreeMap<Offset, Data>,
    pointer_tracker: BTreeSet<Offset>,
}

impl DataItems {
    const fn new() -> Self {
        Self {
            data_items: BTreeMap::new(),
            pointer_tracker: BTreeSet::new(),
        }
    }

    fn lookback(&mut self, pointer: Offset) {
        let mut next_item = None;
        if let Some((offset, data)) = self.data_items.range_mut(..pointer).next_back() {
            trace!("Lookback to {pointer:04x} => {data}");
            match data {
                Data::Ret => todo!(),
                Data::J(_, _) => todo!(),
                Data::Jal(_, _, _, _, _, _) => {}
                Data::Multi(_, _, _) => {}
                Data::TxtPtr(_) => todo!(),
                Data::String(_) => todo!(),
                Data::Cop(_, _, _, _) => todo!(),
                Data::Cop2(_, _, _, _) => todo!(),
                Data::Ptr(_) => todo!(),
                Data::Unmanaged(items) => {
                    next_item = Some(Data::Unmanaged(
                        items.split_off((pointer - offset) as usize),
                    ));
                }
            }
        }
        if let Some(data) = next_item {
            self.data_items.insert(pointer, data);
        }
    }

    fn insert(&mut self, data_offset: Offset, eof: Offset, mut data: Data) {
        assert!(!self.data_items.contains_key(&data_offset), "Tried to overwrite an old offset! They can only be mutated!");
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
                let mut symbol = pointer_symbols.len() as Pointer;
                symbol = *pointer_symbols.entry(data_pointer).or_insert(symbol);
                data.set_pointer_symbol(symbol);
            }
            // If the current offset is referred to by a pointer, create a symbol for it
            if let Some(pointer) = self.pointer_tracker.get(&offset) {
                let symbol = pointer_symbols.len() as Pointer;
                pointer_symbols.entry(*pointer).or_insert(symbol);
            }
            if let Some(symbol) = pointer_symbols.get(&offset) {
                current_section = *symbol;
            }
            if let Some(dialog_item) = dialog_data.remove(&offset) {
                let mut symbol = pointer_symbols.len() as Pointer;
                symbol = *pointer_symbols.entry(offset).or_insert(symbol);
                ordered_dialog.insert(symbol, dialog_item);
            }
            let data_set = ordered_data
                .entry(current_section)
                .or_insert(Vec::with_capacity(16));
            data_set.push(data);
        }

        for (symbol, data_items) in &ordered_data {
            for item in data_items {
                debug!("Symbol: {symbol:04x}, Data: {item}");
            }
        }

        (ordered_data, ordered_dialog)
    }
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

#[derive(Serialize, Deserialize)]
enum BorP {
    Bytes(Vec<u8>),
    Pointer(Pointer),
    Pad,
}

pub fn deserialize_indexmap<'de, D, T>(d: D) -> Result<IndexMap<u32, T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    #[derive(Deserialize, Hash, Eq, PartialEq, Ord, PartialOrd)]
    struct Wrapper(#[serde(deserialize_with = "deserialize_u32_hex")] u32);

    let dict: IndexMap<Wrapper, T> = Deserialize::deserialize(d)?;
    Ok(dict.into_iter().map(|(Wrapper(k), v)| (k, v)).collect())
}

pub fn serialize_indexmap<S, T>(
    s: &IndexMap<u32, T>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
    T: Serialize,
{
    #[derive(Serialize)]
    struct Wrapper<'a>(#[serde(serialize_with = "serialize_u32_hex")] &'a u32);

    let map = s.iter().map(|(k, v)| (Wrapper(k), v));
    serializer.collect_map(map)
}

pub fn save_dialog_strings(
    path: &PathBuf,
    dialog: &IndexMapWrapper<DialogString>,
) -> Result<(), io::Error> {
    let strings = toml::to_string(&dialog).unwrap();

    let f = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(path)?;
    let mut f = BufWriter::new(f);
    f.write_all(strings.as_bytes())?;
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

pub fn load_dialog_strings<P: AsRef<Path>>(path: P) -> Result<OrderedDialog, io::Error> {
    let f = OpenOptions::new().read(true).open(path)?;
    let mut string = String::with_capacity(f.metadata().unwrap().len() as usize);
    let mut f = BufReader::new(f);
    f.read_to_string(&mut string)?;
    Ok(toml::from_str::<IndexMapWrapper<DialogString>>(&string)
        .unwrap()
        .0)
}

// pub(crate) fn load_event_data(path: &PathBuf) -> Result<OrderedData, io::Error> {
//     let f = OpenOptions::new().read(true).open(path)?;
//     let mut string = String::with_capacity(f.metadata().unwrap().len() as usize);
//     let mut f = BufReader::new(f);
//     f.read_to_string(&mut string)?;
//     Ok(serde_json::from_str::<IndexMapWrapper<Vec<Data>>>(&string)?.0)
// }

#[derive(Serialize, Deserialize)]
pub struct IndexMapWrapper<T: Serialize + DeserializeOwned>(
    #[serde(
        deserialize_with = "deserialize_indexmap",
        serialize_with = "serialize_indexmap"
    )]
    pub(crate) IndexMap<u32, T>,
);

pub fn rebuild_event<P: AsRef<Path>>(
    data: &[u8],
    file_name: &str,
    dialog_file_path: P,
) -> Result<Vec<u8>, io::Error> {
    let ordered_data = serde_json::from_slice::<IndexMapWrapper<Vec<Data>>>(data)?.0;
    #[expect(clippy::if_then_some_else_none, reason = "Closure would require unwrapping")]
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

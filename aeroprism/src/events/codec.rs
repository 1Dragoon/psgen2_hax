extern crate alloc;
use crate::{
    events::{
        Archy, BytesOrPointer, Color, ControlCode, Data, DataItems, DialogItem, DialogString,
        GUESTIMATED_LENGTH, MTECode, Offset, Pointer, Portrait, UmanagedData,
        sjis_map::{SJIS_STARTER_BYTES, byte_to_engrish, byte_to_sjis, word_to_sjis},
    },
    helpers::{encode_hex, hex_edit_encode},
};
use alloc::{
    collections::BTreeMap,
    sync::Arc,
    vec::{IntoIter, Vec},
};
use byteorder::ReadBytesExt;
use core::{iter::Peekable, mem, ops::Bound};
use indexmap::IndexMap;
use log::{Level, debug, error, log_enabled, trace, warn};
use snafu::prelude::*;
use std::{
    collections::HashMap,
    io::{BufRead, Seek, SeekFrom},
    sync::RwLock,
};
use tokio::io;

pub type OrderedData = IndexMap<Pointer, Vec<Data>>;
pub type DialogMap = BTreeMap<Pointer, DialogString>;
pub type OrderedDialog = IndexMap<Pointer, DialogString>;

#[derive(Debug, Snafu)]
pub enum SjisError {
    #[snafu(display("Unexpected character code: 0x{byte:02x}"))]
    UnexpectedCharacter { byte: u8 },

    #[snafu(display("Unexpected double character code: 0x{byte:02x}{next_byte:02x}"))]
    UnexpectedDoubleCharacter { byte: u8, next_byte: u8 },

    #[snafu(display(
        "Expected another character to follow a SHIFTJIS double character, but the data is truncated. Last byte 0x{byte:02x}"
    ))]
    UnexpectedEof { byte: u8 },
}

#[expect(clippy::single_call_fn, reason = "readability")]
pub fn parse_events<R: Seek + BufRead>(
    reader: &mut R,
    eof: Offset,
) -> Result<(OrderedData, OrderedDialog), io::Error> {
    #[expect(
        unused_assignments,
        reason = "it does get used, only can be written to before then, but this is fine."
    )]
    let mut current_section = 0;
    let mut string_offsets: BTreeMap<Offset, Archy> = BTreeMap::new();
    // Initialize with offset zero
    let mut data_items = DataItems::new();
    let mut current_u32 = [0u8; 4];
    let mut current_unmanaged = UmanagedData::new();
    loop {
        let current_offset = Offset::try_from(reader.stream_position()?).unwrap();
        if data_items.pointer_tracker.contains(&current_offset) {
            current_unmanaged.finish(eof, &mut data_items);
        }
        // println!("Current offset: {current_offset:04x}");
        if current_offset == eof {
            // Yay, we're done!
            current_unmanaged.finish(eof, &mut data_items);
            break;
        }
        if let Some(next_pointer) = data_items
            .pointer_tracker
            .get(&(current_offset as Offset))
            .copied()
        {
            current_section = next_pointer;
            // If we've hit an offset that a text pointer indicates
            if let Some(string_bytes) = string_offsets.get(&current_section) {
                current_unmanaged.finish(eof, &mut data_items);
                data_items.insert(current_offset, eof, Data::String(Arc::clone(string_bytes)));
                // Let's jump to the lesser of the end of the string, or the next offset that any pointer references
                let maybe_next_offset = data_items
                    .pointer_tracker
                    .range((Bound::Excluded(current_offset), Bound::Unbounded))
                    .next()
                    .copied();
                let jump_to = maybe_next_offset
                    .unwrap_or(eof)
                    .min(Offset::try_from(string_bytes.read().unwrap().len()).unwrap());
                reader.seek_relative(i64::from(jump_to))?;
                continue;
            }
        }
        let misaligned = !current_offset.is_multiple_of(4);
        if misaligned {
            warn!("\nWe're at misaligned current offset {current_offset:04x}");
        }
        let bytes_remaining = eof.saturating_sub(current_offset);
        // println!("Bytes remaining: {}", bytes_remaining);
        if bytes_remaining >= u32::try_from(current_u32.len()).unwrap() {
            reader.read_exact(&mut current_u32)?;
            match current_u32 {
                [op, 0x00, _, 0x00]
                    if !misaligned
                        && [0x26, 0x2a, 0x2b, 0x2e, 0x40, 0x42, 0x44, 0x4d, 0x54].contains(&op) =>
                {
                    current_unmanaged.update(current_offset, current_u32);
                    if [0x2a, 0x2e, 0x40, 0x42, 0x44, 0x4d, 0x54].contains(&op) {
                        reader.read_exact(&mut current_u32)?;
                        current_unmanaged.update(current_offset, current_u32);
                    }
                }
                [0x0a, 0x00, 0x00, 0x00] if !misaligned => {
                    current_unmanaged.finish(eof, &mut data_items);
                    data_items.insert(current_offset, eof, Data::Ret);
                }
                // [0x1e, 0x00, 0x00, 0x00]
                //     if !misaligned
                //         && matches!(current_unmanaged.state, UnmanagedDataState::Idle) => {
                //     reader.read_exact(&mut current_u32)?;
                //     let pointer = u32::from_le_bytes(current_u32);
                //     data_items.insert(current_offset, eof, Data::J(0x1e, pointer));
                // }
                [op, 0x00, 0x00, 0x00]
                    if !misaligned
                        // && matches!(current_unmanaged.state, UnmanagedDataState::Idle)
                        && [0x0b, 0x1e, 0x17].contains(&op) =>
                {
                    current_unmanaged.finish(eof, &mut data_items);
                    reader.read_exact(&mut current_u32)?;
                    let pointer = u32::from_le_bytes(current_u32);
                    data_items.insert(current_offset, eof, Data::J(op, pointer));
                }
                [0x0c, 0x00, c, d] if !misaligned && c > 0 => {
                    current_unmanaged.finish(eof, &mut data_items);
                    reader.read_exact(&mut current_u32)?;
                    let field_1 = u32::from_le_bytes(current_u32);
                    reader.read_exact(&mut current_u32)?;
                    let field_2 = u32::from_le_bytes(current_u32);
                    reader.read_exact(&mut current_u32)?;
                    let pointer = u32::from_le_bytes(current_u32);
                    data_items.insert(
                        current_offset,
                        eof,
                        Data::Jal(0x0c, c, d, field_1, field_2, pointer),
                    );
                }
                [0x38, 00, 00, 00] if !misaligned => {
                    current_unmanaged.finish(eof, &mut data_items);
                    reader.read_exact(&mut current_u32)?;
                    let pointer = u32::from_le_bytes(current_u32);
                    data_items.insert(current_offset, eof, Data::J(0x38, pointer));
                    // This one has a second pointer
                    reader.read_exact(&mut current_u32)?;
                    let second_pointer = u32::from_le_bytes(current_u32);
                    data_items.insert(current_offset + 4, eof, Data::Ptr(second_pointer));
                }
                [op, 0x00, c, 0x00] if !misaligned && [0x0f, 0x10].contains(&op) => {
                    current_unmanaged.finish(eof, &mut data_items);
                    reader.read_exact(&mut current_u32)?;
                    let pointer = u32::from_le_bytes(current_u32);
                    let mut values = Vec::with_capacity(c.into());
                    for _ in 0..c {
                        reader.read_exact(&mut current_u32)?;
                        let value = u32::from_le_bytes(current_u32);
                        values.push(value);
                    }
                    data_items.insert(current_offset, eof, Data::Multi(op, pointer, values));
                }
                [0x12, 0x00, 0x00, 0x00] if !misaligned => {
                    current_unmanaged.finish(eof, &mut data_items);
                    reader.read_exact(&mut current_u32)?;
                    let pointer = u32::from_le_bytes(current_u32);
                    data_items.insert(current_offset, eof, Data::TxtPtr(pointer));

                    if eof > pointer {
                        if fast_forward(&mut string_offsets, pointer) {
                            continue;
                        }

                        let pos_before_text_jump =
                            scan_to_terminator(reader, eof, &string_offsets, pointer)?;
                        let string_end_offset =
                            Offset::try_from(reader.stream_position()?).unwrap();
                        let string_length = string_end_offset - pointer;

                        let mut string_bytes = vec![0; string_length as usize];
                        reader.seek(SeekFrom::Start(u64::from(pointer)))?;
                        reader.read_exact(&mut string_bytes)?;
                        let rc_string_bytes = Arc::new(RwLock::new(string_bytes));
                        string_offsets.insert(pointer, rc_string_bytes);
                        trace!("Jumping back to {pos_before_text_jump:04x}");
                        reader.seek(SeekFrom::Start(pos_before_text_jump))?;
                    }
                }
                [op, 0x00, c, d] if !misaligned && [0x41, 0x24, 0x25].contains(&op) => {
                    current_unmanaged.finish(eof, &mut data_items);
                    reader.read_exact(&mut current_u32)?;
                    let pointer = u32::from_le_bytes(current_u32);
                    data_items.insert(current_offset, eof, Data::Cop(op, c, d, pointer));
                }
                [op, 0x00, c, 0x00] if !misaligned && [0x33, 0x4a].contains(&op) => {
                    current_unmanaged.finish(eof, &mut data_items);
                    reader.read_exact(&mut current_u32)?;
                    let field = u32::from_le_bytes(current_u32);
                    reader.read_exact(&mut current_u32)?;
                    let pointer = u32::from_le_bytes(current_u32);
                    data_items.insert(current_offset, eof, Data::Cop2(op, c, field, pointer));
                }
                other => {
                    current_unmanaged.update(current_offset, other);
                }
            }
        } else {
            current_unmanaged.finish(eof, &mut data_items);
            let mut data = Vec::with_capacity(current_u32.len());
            reader.read_to_end(&mut data)?;
            if !data.is_empty() {
                error!(
                    "Ended on uneven boundary. Output is truncated. Data: {}",
                    encode_hex(&data)
                );
            }
            break;
        }
    }
    if log_enabled!(Level::Debug) {
        let mut debug_string = String::with_capacity(GUESTIMATED_LENGTH);
        debug_string.push_str("Pointer tracker has: \n");
        for pointer in &data_items.pointer_tracker {
            debug_string.push_str(format!("0x{pointer:04x} ").as_str());
        }
        debug!("{debug_string}");
    }

    let mut dialog_items = BTreeMap::new();
    for (pointer, string) in string_offsets {
        let string_end_offset = Offset::try_from(string.read().unwrap().len()).unwrap() + pointer;
        if log_enabled!(Level::Trace) {
            trace!("{}", hex_edit_encode(&string.read().unwrap()));
        }

        let string_repr = decode_psg2_string(mem::take(&mut *string.write().unwrap()));
        drop(string);

        if log_enabled!(Level::Debug) {
            let mut debug_string = String::with_capacity(GUESTIMATED_LENGTH);
            debug_string.push_str(format!("-----begin string---- {:04x}\n", &pointer).as_str());
            for item in &string_repr.text {
                debug_string.push_str(format!("{item}").as_str());
            }
            debug_string
                .push_str(format!("\n-----end string----- {string_end_offset:04x}").as_str());
            if string_repr.padded {
                debug_string.push_str(" (padded)");
            }
            debug_string.push('\n');
            debug!("{debug_string}");
        }
        dialog_items.insert(pointer, string_repr);
    }

    Ok(data_items.into_ordered_data(dialog_items))
}

#[inline]
#[expect(clippy::single_call_fn, reason = "readability")]
pub fn marshal_events(
    // original_data: &[u8],
    ordered_data: OrderedData,
    mut dialog_items: Option<OrderedDialog>,
    file_name: &str,
) -> Vec<u8> {
    let mut offset_tracker: HashMap<Pointer, Offset> = HashMap::new();
    let mut est_offset: usize = 0;
    for (pointer, data) in &ordered_data {
        #[expect(
            clippy::panic,
            reason = "these are totally irrecoverable failures -- guaranteed corrupt outputs"
        )]
        for datum in data {
            offset_tracker
                .entry(*pointer)
                .or_insert_with(|| Offset::try_from(est_offset).unwrap());
            let offset = offset_tracker
                .get(pointer)
                .unwrap_or_else(|| panic!("Fatal error: Missing pointer object {pointer:04x}"));

            if let Data::String(string) = &datum {
                let dialog_string = dialog_items
                    .as_mut()
                    .unwrap_or_else(|| {
                        panic!("Fatal error: No dialog strings provided for this file!")
                    })
                    .swap_remove(pointer)
                    .unwrap_or_else(|| {
                        panic!("Fatal error: Missing dialog pointer object {pointer:04x}")
                    });
                let string_bytes = dialog_string.into_bytes(Some(est_offset));
                *string.write().unwrap() = string_bytes;
            }
            if log_enabled!(Level::Trace) {
                trace!("^{file_name}: [{est_offset:04x}] ({offset:04x}) {datum}");
            }
            est_offset += datum.len();
        }
    }

    debug!("Offset tracker is {} items", offset_tracker.len());

    let mut coalesced_data = coalesce_bytes(ordered_data);
    coalesced_data.shrink_to_fit();

    // Write the reconstituted event file out
    let mut data_out = Vec::with_capacity(est_offset + 512);
    // let mut stop_compare_oputput = false;
    for (section_pointer, pointer_data) in coalesced_data {
        for pointer_section in pointer_data {
            match pointer_section {
                BytesOrPointer::PadBytes => {
                    while (data_out.len() % 4) != 0 {
                        data_out.push(0);
                    }
                }
                BytesOrPointer::Bytes(bytes) => {
                    data_out.extend(bytes);
                }
                BytesOrPointer::Pointer(pointer) => {
                    if let Some(offset) = offset_tracker.get(&pointer) {
                        let bytes = (*offset).to_le_bytes();
                        data_out.extend(bytes);
                    } else {
                        let msg = format!(
                            "Got invalid pointer ({pointer:04x}) while in ({section_pointer:04x}) at [{:04x}]",
                            data_out.len()
                        );
                        error!("{msg}");
                    }
                }
            }
            // if data_out != original_data[..data_out.len()] && !stop_compare_oputput {
            //     let mut start_compare = 0;
            //     for (our_bytes, orig_bytes) in data_out.chunks(16).zip(original_data.chunks(16)) {
            //         start_compare += 16;
            //         if our_bytes != orig_bytes {
            //             error!("Beginning at: [{start_compare:04x}]");
            //             error!("ours:   {}", encode_hex(our_bytes));
            //             error!("theirs: {}", encode_hex(orig_bytes));
            //             error!("Data is corrupted!!");
            //             break;
            //         }
            //         stop_compare_oputput = true;
            //     }
            // }
        }
    }
    data_out.shrink_to_fit();
    data_out
}

fn debug_raw_string(raw_ps2_sjis_string: &[u8]) {
    let string_repr = decode_psg2_string(raw_ps2_sjis_string.to_vec());
    let mut debug_string = String::with_capacity(GUESTIMATED_LENGTH);
    debug_string.push_str("-----begin string---- \n");
    for item in &string_repr.text {
        debug_string.push_str(format!("{item}").as_str());
    }
    debug_string.push_str("\n-----end string-----");
    if string_repr.padded {
        debug_string.push_str(" (padded)");
    }
    debug_string.push('\n');
    trace!("{debug_string}");
}

pub fn decode_psg2_string(mut raw_ps2_sjis_string: Vec<u8>) -> DialogString {
    // Remove any null bytes at the end
    let mut i = 0;
    let pad = raw_ps2_sjis_string.last().is_some_and(|v| *v == 0);
    while raw_ps2_sjis_string.last().is_some_and(|v| *v == 0) {
        i += 1;
        raw_ps2_sjis_string.pop();
    }
    trace!("Removed {i} null padding byte(s).");

    let mut string_iter = raw_ps2_sjis_string.into_iter().peekable();
    let mut dialog_string = Vec::<DialogItem>::with_capacity(16);
    while let Some(byte) = string_iter.next() {
        // Try for MTE codes first
        if [0x09, 0x10, 0x11, 0x12, 0x13].contains(&byte)
            && let Some(next_byte) = string_iter.peek()
        {
            let word = u16::from_be_bytes([byte, *next_byte]);
            let mc = MTECode::from(word);
            // If this is a valid MTE code, store it and continue to the next byte
            if !matches!(mc, MTECode::None) {
                dialog_string.push(DialogItem::MTECode(mc));
                string_iter.next().unwrap();
                continue;
            }
        }
        // Then try for control codes
        let cc = ControlCode::from(byte);
        match cc {
            ControlCode::Armel
            | ControlCode::Armor
            | ControlCode::Bandana
            | ControlCode::Boots
            | ControlCode::Cake
            | ControlCode::Cane
            | ControlCode::Cannon
            | ControlCode::Chestplate
            | ControlCode::Circle
            | ControlCode::Claw
            | ControlCode::Coat
            | ControlCode::Cross
            | ControlCode::Crown
            | ControlCode::Dagger
            | ControlCode::Espadrilles
            | ControlCode::Fibrillae // Careful with this
            | ControlCode::Fluid
            | ControlCode::Gun
            | ControlCode::Hat
            | ControlCode::Headgear
            | ControlCode::Helmet
            | ControlCode::Important
            | ControlCode::Knife
            | ControlCode::Mantle
            | ControlCode::Mantle2
            | ControlCode::Heal
            | ControlCode::Moon
            | ControlCode::Musik
            | ControlCode::Ocarina
            | ControlCode::Ribbon
            | ControlCode::Scale
            | ControlCode::Scalpel
            | ControlCode::Shield
            | ControlCode::Shoes
            | ControlCode::Shot
            | ControlCode::Slicer
            | ControlCode::Sol
            | ControlCode::Square
            | ControlCode::Star
            | ControlCode::Suit
            | ControlCode::Sword
            | ControlCode::Triangle
            | ControlCode::Vest
            | ControlCode::Vulcan
            | ControlCode::Whip
            | ControlCode::Push
            | ControlCode::End
            | ControlCode::More
            | ControlCode::Select
            | ControlCode::CCVariant1
            | ControlCode::CCVariant3
            | ControlCode::CCVariant2
            | ControlCode::Megiddo
            | ControlCode::Fire
            | ControlCode::Gravito
            | ControlCode::Water
            | ControlCode::Air
            | ControlCode::Lightning
            | ControlCode::Volt
            | ControlCode::Light
            | ControlCode::Prozedun
            | ControlCode::Value => dialog_string.push(DialogItem::ControlCode(cc)),
            ControlCode::Color => {
                if let Some(number) = string_iter.next_if(u8::is_ascii_digit) {
                    dialog_string.push(DialogItem::Color(Color::from(number)));
                } else {
                    dialog_string.push(DialogItem::ControlCode(ControlCode::Fibrillae));
                }
            }
            ControlCode::Portrait => {
                let mut portrait_numerals = Vec::with_capacity(2);
                while let Some(number) = string_iter.next_if(u8::is_ascii_digit) {
                    portrait_numerals.push(number);
                }
                let val = DialogItem::Portrait(Portrait(
                    String::from_utf8(portrait_numerals)
                        .unwrap_or_else(|err| {warn!("Error decoding portrait numeral: {err}\n I'll give you a motavian instead."); "86".into()}),
                ));
                dialog_string.push(val);
            }
            ControlCode::None => {
                // Just try normal strings from here
                let mut sjis_strings = Vec::with_capacity(40);
                parse_next_event_char(&mut string_iter, &mut sjis_strings, byte);
                while let Some(next_string_byte) = string_iter.next_if(|b| {
                    *b == b'@' || *b == b' ' || SJIS_STARTER_BYTES.binary_search(b).is_ok()
                }) {
                    parse_next_event_char(&mut string_iter, &mut sjis_strings, next_string_byte);
                }

                // let decoded_sjis_string = decode_string(&sjis_string).to_string();
                if let Some(DialogItem::String(eis)) = dialog_string.last_mut() {
                    for string in sjis_strings {
                        eis.push_str(string.as_str());
                    }
                } else {
                    dialog_string.push(DialogItem::String(sjis_strings.concat()));
                }
            }
        }
    }
    dialog_string.shrink_to_fit();
    DialogString {
        text: dialog_string,
        padded: pad,
    }
}

// Identify where any string boundaries may lie, splitting where necessary.
fn fast_forward(string_offsets: &mut BTreeMap<u32, Archy>, pointer: u32) -> bool {
    if string_offsets.contains_key(&pointer) {
        // Just a reference to an existing string. There's no need to do anything that we haven't already done.
        return true;
    }

    // This is either a new string or is a substring of another string. Let's find out!
    if let Some((prev_offset, existing_string)) = string_offsets.range(..&pointer).next_back() {
        // If we're within the bounds of an existing string, then let's split it at the pointer
        if (pointer - prev_offset)
            < Offset::try_from(existing_string.read().unwrap().len()).unwrap()
        {
            if log_enabled!(Level::Trace) {
                trace!("Before:");
                debug_raw_string(&existing_string.read().unwrap());
                trace!(
                    "\nRaw bytes:\n{}",
                    hex_edit_encode(&existing_string.read().unwrap()).to_uppercase()
                );
            }
            let new_string = existing_string
                .write()
                .unwrap()
                .split_off((pointer - prev_offset) as usize);
            if log_enabled!(Level::Trace) {
                trace!("After:");
                debug_raw_string(&existing_string.read().unwrap());
                trace!(
                    "\nRaw bytes:\n{}",
                    hex_edit_encode(&existing_string.read().unwrap()).to_uppercase()
                );
                debug_raw_string(&new_string);
                trace!(
                    "\nRaw bytes:\n{}",
                    hex_edit_encode(&new_string).to_uppercase()
                );
                trace!(
                    "The new string ends at {:04x}",
                    new_string.len() + (pointer as usize)
                );
            }
            string_offsets.insert(pointer, Arc::new(RwLock::new(new_string)));

            // The string is now split off and has its own pointer. There's no need to do anything else.
            return true;
        }
    }
    false
}

fn scan_to_terminator<R: Seek + BufRead>(
    reader: &mut R,
    eof: u32,
    string_offsets: &BTreeMap<u32, Archy>,
    pointer: u32,
) -> Result<u64, io::Error> {
    let pos_before_text_jump = reader.stream_position()?;
    trace!("Jumping to offset {pointer:04x} from {pos_before_text_jump:04x}");
    reader.seek(SeekFrom::Start(u64::from(pointer)))?;
    let mut previous_byte = 0;
    let mut max_string_length = if let Some((next_pointer, _)) = string_offsets
        .range((Bound::Excluded(pointer), Bound::Unbounded))
        .next()
    {
        *next_pointer - pointer
    } else {
        eof - pointer
    };
    while max_string_length > 0
        && let Ok(byte) = reader.read_u8()
        && byte != 0x00
    {
        previous_byte = byte;
        max_string_length -= 1;
        // Seek until we run into a null character, which may or may not be a terminator.
        // Or, alternatively, the lesser of the next string offset or EOF.
    }
    if max_string_length > 0 {
        find_terminator_offset(reader, eof, pointer, previous_byte)?;
    } else {
        trace!("Saved by the max string length! [{pointer:04x}]");
    }
    Ok(pos_before_text_jump)
}

fn find_terminator_offset<R: Seek + BufRead>(
    reader: &mut R,
    eof: u32,
    pointer: u32,
    previous_byte: u8,
) -> Result<(), io::Error> {
    if [
        0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10, 0x12, 0x24, 0x25, 0x32, 0x34,
    ]
    .contains(&previous_byte)
    {
        // The terminator is the character before this, so set the cursor immediately after it.
        reader.seek_relative(-2)?;
    } else if ![0x2a, 0x5c].contains(&previous_byte) {
        let end_offset = reader.stream_position()?;
        error!(
            "Byte before string terminator at [{end_offset:04x}] is {previous_byte:02x} for string beginning at offset [{pointer:04x}]"
        );
    }
    let terminator_offset = Offset::try_from(reader.stream_position()?).unwrap();
    let mut text_ending_alignment = 4 - (terminator_offset % 4);
    if text_ending_alignment < 4 && text_ending_alignment > 0 {
        // Advance to the end of any zero padding
        while let Ok(byte) = reader.read_u8()
            && byte == 0x00
            && text_ending_alignment > 0
        {
            text_ending_alignment -= 1;
        }
        if Offset::try_from(reader.stream_position()?).unwrap() != eof {
            // We found where the padding ended, which isn't at EOF, so go back to the previous offset.
            reader.seek_relative(-1)?;
        }
    }
    Ok(())
}

#[track_caller]
pub fn parse_next_event_char(
    string_iter: &mut Peekable<IntoIter<u8>>,
    sjis_string: &mut Vec<String>,
    byte: u8,
) {
    if byte == b' ' {
        sjis_string.push(" ".to_owned());
    } else if byte == b'@' {
        sjis_string.push("\n".to_owned());
    } else {
        match parse_next_sjis(string_iter, sjis_string, byte) {
            Ok(_) => (),
            Err(err) => {
                warn!("{err}");
                match err {
                    SjisError::UnexpectedDoubleCharacter {
                        byte: unexpected,
                        next_byte,
                    } => {
                        sjis_string.push(format!("x{unexpected:02x}x{next_byte:02x}"));
                        string_iter.next().unwrap();
                    }
                    SjisError::UnexpectedCharacter { byte: unexpected }
                    | SjisError::UnexpectedEof { byte: unexpected } => {
                        sjis_string.push(format!("x{unexpected:02x}"));
                    }
                }
            }
        }
    }
}

#[track_caller]
pub fn parse_next_sjis(
    string_iter: &mut Peekable<IntoIter<u8>>,
    sjis_string: &mut Vec<String>,
    byte: u8,
) -> Result<u8, SjisError> {
    // C string formatter codes
    if byte == b'%' {
        let next_byte = string_iter
            .peek()
            .copied()
            .context(UnexpectedEofSnafu { byte })?;
        match next_byte {
            b'c'..=b'f' | b'u' | b'o' | b'x' | b'X' | b'E' | b's' | b'*' | b'0'..=b'9' => {
                let char = String::from_utf8_lossy(&[next_byte]).into_owned();
                sjis_string.push(format!("%{char}"));
                string_iter.next().unwrap();
                return Ok(2);
            }
            _ => (),
        }
    }
    if *crate::ENGRISH.get().unwrap()
        && let Some(string) = byte_to_engrish(byte)
    {
        sjis_string.push(string.into());
        return Ok(1);
    }
    if let Some(string) = byte_to_sjis(byte) {
        sjis_string.push(string.into());
        return Ok(1);
    }

    let next_byte = string_iter
        .peek()
        .copied()
        .context(UnexpectedEofSnafu { byte })?;
    let character = word_to_sjis([byte, next_byte])
        .context(UnexpectedDoubleCharacterSnafu { byte, next_byte })?;
    sjis_string.push(character.into());
    string_iter.next().unwrap();
    Ok(2)
}

fn coalesce_bytes(
    ordered_data: IndexMap<Pointer, Vec<Data>>,
) -> IndexMap<Pointer, Vec<BytesOrPointer>> {
    let mut chunked_data: IndexMap<Pointer, Vec<BytesOrPointer>> =
        IndexMap::with_capacity(ordered_data.len() * 2);
    for (pointer, pre_chunked_data) in ordered_data {
        for datum in pre_chunked_data {
            match datum {
                Data::Ret => {
                    let data = chunked_data
                        .entry(pointer)
                        .or_insert(Vec::with_capacity(40));
                    data.push(BytesOrPointer::Bytes(vec![0x0a, 0x00, 0x00, 0x00]));
                }
                Data::J(op, ref_pointer) => {
                    let data = chunked_data
                        .entry(pointer)
                        .or_insert(Vec::with_capacity(40));
                    data.push(BytesOrPointer::Bytes(vec![op, 0x00, 0x00, 0x00]));
                    data.push(BytesOrPointer::Pointer(ref_pointer));
                }
                Data::Jal(op, c, d, f1, f2, ref_pointer) => {
                    let data = chunked_data
                        .entry(pointer)
                        .or_insert(Vec::with_capacity(40));
                    let op_bytes = [op, 0x00, c, d];
                    let mut bytes =
                        Vec::with_capacity(op_bytes.len() + size_of_val(&f1) + size_of_val(&f2));
                    bytes.extend(op_bytes);
                    bytes.extend(f1.to_le_bytes());
                    bytes.extend(f2.to_le_bytes());

                    data.push(BytesOrPointer::Bytes(bytes));
                    data.push(BytesOrPointer::Pointer(ref_pointer));
                }
                Data::Multi(op, ref_pointer, items) => {
                    let data = chunked_data
                        .entry(pointer)
                        .or_insert(Vec::with_capacity(40));
                    let op_bytes = vec![op, 0x00, u8::try_from(items.len()).unwrap(), 0x00];
                    data.push(BytesOrPointer::Bytes(op_bytes));
                    data.push(BytesOrPointer::Pointer(ref_pointer));
                    let mut bytes = Vec::with_capacity(size_of::<u32>() * items.len());
                    for item in items {
                        bytes.extend(item.to_le_bytes());
                    }
                    data.push(BytesOrPointer::Bytes(bytes));
                }
                Data::TxtPtr(ref_pointer) => {
                    let data = chunked_data
                        .entry(pointer)
                        .or_insert(Vec::with_capacity(40));
                    let op_bytes = vec![0x12, 0x00, 0x00, 0x00];
                    data.push(BytesOrPointer::Bytes(op_bytes));
                    data.push(BytesOrPointer::Pointer(ref_pointer));
                }
                Data::String(string) => {
                    let data = chunked_data
                        .entry(pointer)
                        .or_insert(Vec::with_capacity(40));
                    data.push(BytesOrPointer::Bytes(mem::take(
                        &mut string.write().unwrap(),
                    )));
                }
                Data::Cop(op, c, d, ref_pointer) => {
                    let data = chunked_data
                        .entry(pointer)
                        .or_insert(Vec::with_capacity(40));
                    let op_bytes = vec![op, 0x00, c, d];
                    data.push(BytesOrPointer::Bytes(op_bytes));
                    data.push(BytesOrPointer::Pointer(ref_pointer));
                }
                Data::Cop2(op, c, field, ref_pointer) => {
                    let data = chunked_data
                        .entry(pointer)
                        .or_insert(Vec::with_capacity(40));
                    let op_bytes = vec![op, 0x00, c, 0x00];
                    let mut bytes = Vec::with_capacity(
                        op_bytes.len() + size_of_val(&field) + size_of_val(&ref_pointer),
                    );
                    bytes.extend(op_bytes);
                    bytes.extend(field.to_le_bytes());
                    data.push(BytesOrPointer::Bytes(bytes));
                    data.push(BytesOrPointer::Pointer(ref_pointer));
                }
                Data::Ptr(ref_pointer) => {
                    let data = chunked_data
                        .entry(pointer)
                        .or_insert(Vec::with_capacity(40));
                    data.push(BytesOrPointer::Pointer(ref_pointer));
                }
                Data::Unmanaged(bytes) => {
                    let data = chunked_data
                        .entry(pointer)
                        .or_insert(Vec::with_capacity(40));
                    data.push(BytesOrPointer::Bytes(bytes));
                }
            }
        }
    }
    chunked_data
}

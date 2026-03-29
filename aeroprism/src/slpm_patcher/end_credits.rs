#![allow(clippy::arbitrary_source_item_ordering, reason = "not needed")]
use crate::{
    events::{DialogString, codec::decode_psg2_string},
    helpers::hex_edit_encode,
};
use alloc::collections::BTreeMap;
use core::mem::size_of;
use log::{Level, debug, log_enabled, warn};
use serde::{Deserialize, Serialize};
use std::io;
use tokio::{
    fs::{self},
    io::{AsyncBufRead, AsyncReadExt, AsyncSeek, AsyncSeekExt, AsyncWriteExt, BufWriter, SeekFrom},
};

static END_CREDITS_START: usize = 0x1A_0ED4;
static END_CREDITS_END: usize = 0x1A_1C6C;
static END_CREDITS_BLOB_SIZE: usize = END_CREDITS_END - END_CREDITS_START;
static CREDIT_ITEM_HEADER_SIZE: usize = size_of::<u32>() * 2;
static CREDIT_FOOTER: [u8; CREDIT_ITEM_HEADER_SIZE] =
    [0x01, 0x00, 0x2c, 0x01, 0x00, 0x00, 0x00, 0x00];

#[derive(Serialize, Deserialize)]
pub struct EndCreditItem {
    vertical_space: u16,
    #[serde(flatten)]
    credit_string: DialogString,
}

#[inline]
pub async fn parse<R: AsyncBufRead + AsyncSeek + Unpin>(
    reader: &mut R,
) -> Result<BTreeMap<usize, EndCreditItem>, io::Error> {
    reader
        .seek(SeekFrom::Start(END_CREDITS_START as u64))
        .await?;

    let mut credit_items = BTreeMap::new();
    let mut field = [0x0; 2];
    let mut i = 0;
    debug!("Parsing end credits...");
    loop {
        i += 1;
        // The way each "credit header" appears to work is:
        // 01000XXXX 0200YYYY
        // - XXXX is a 16-bit number to indicate how far we should scroll before displaying the string that follows.
        // - YYYY is a 16-bit number to indicate the length in bytes PLUS the first null terminator of the string to
        // display
        // The string PLUS null terminator that follows then must be padded to the next 32-bit boundary.

        // Move the cursor past the 0x0100 marker
        reader.read_exact(&mut field).await?;
        // Read the vertical space number
        reader.read_exact(&mut field).await?;
        let vertical_space = u16::from_le_bytes(field);
        // Read the 0x0200 marker
        reader.read_exact(&mut field).await?;
        let second_marker = u16::from_le_bytes(field);
        if second_marker != 2 {
            // If the 0x0200 marker is 0x0000, that is the signal to display the "THE END" graphic after scrolling the
            // vertical space distance in the final header.
            debug!("Ended on {i}th credit.");
            break;
        }
        // Move the cursor past the length indicator -- we only need to calculate it dynamically upon patching.
        reader.read_exact(&mut field).await?;

        // Read all of the bytes until the first null terminator
        let mut string_bytes = Vec::with_capacity(32);
        while let Ok(byte) = reader.read_u8().await
            && byte != 0
        {
            string_bytes.push(byte);
        }
        // Move the cursor to the start of the next field

        while reader.stream_position().await? % 4 != 0 {
            reader.read_u8().await?;
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
        let credit_string = decode_psg2_string(string_bytes);
        if log_enabled!(Level::Debug) {
            debug!("Rendered credit string: {credit_string}");
            debug!("Debugged credit string: {credit_string:#?}",);
        }
        // Read the next two fields
        credit_items.insert(
            i,
            EndCreditItem {
                vertical_space,
                credit_string,
            },
        );
    }
    Ok(credit_items)
}

#[inline]
pub async fn patch(
    exec_writer: &mut BufWriter<fs::File>,
    end_credits: BTreeMap<usize, EndCreditItem>,
) -> Result<(), io::Error> {
    exec_writer
        .seek(SeekFrom::Start(END_CREDITS_START.try_into().unwrap()))
        .await?;
    let mut total_bytes = 0;
    for (_, end_credit_item) in end_credits {
        let EndCreditItem {
            vertical_space,
            mut credit_string,
        } = end_credit_item;

        if log_enabled!(Level::Debug) {
            debug!("Debugged credit string: {credit_string:#?}");
        }

        // The header wants the string byte length plus the null terminator
        let credit_string_size = u16::try_from(credit_string.byte_len() + 1).unwrap();
        let credit_header = [
            1u16.to_le_bytes(),
            vertical_space.to_le_bytes(),
            2u16.to_le_bytes(),
            credit_string_size.to_le_bytes(),
        ]
        .concat();
        // Mark as padded so we don't have to calculate that manually here
        credit_string.set_padded();

        // Convert the string into bytes and calculate the length field, storing as a u16 for later
        let expand_by = credit_string.byte_len() + credit_header.len();
        if expand_by + total_bytes + CREDIT_ITEM_HEADER_SIZE > END_CREDITS_BLOB_SIZE {
            if log_enabled!(Level::Warn) {
                warn!(
                    "End credit overflow! Data corruption likely! Overflowed by {} bytes. Stopping at text '{}'",
                    (expand_by + total_bytes + CREDIT_ITEM_HEADER_SIZE)
                        .saturating_sub(END_CREDITS_BLOB_SIZE),
                    credit_string
                );
            }
            break;
        }
        let string_bytes = credit_string.into_bytes(None);

        if log_enabled!(Level::Debug) {
            debug!("Rendered credit string: {}", hex_edit_encode(&string_bytes));
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

// use crate::helpers::{deserialize_u32_hex, encode_hex, serialize_u32_hex};
// use log::warn;
use crate::{
    events::{DialogString, codec::decode_psg2_string},
    helpers::Hexu32,
    slpm_patcher::{POINTER_OFFSET, RelativePointerInfo, StringFill},
};
use alloc::collections::BTreeMap;
use serde::{Deserialize, Serialize};
use std::io;
use tokio::{
    fs,
    io::{
        AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncSeek, AsyncSeekExt, AsyncWriteExt,
        BufWriter, SeekFrom,
    },
};

#[expect(
    clippy::arbitrary_source_item_ordering,
    reason = "Ordered by binary struct fields."
)]
#[derive(Serialize, Deserialize)]
pub struct JumplistItem {
    #[serde(flatten)]
    pub string: DialogString,
    // #[serde(
    //     serialize_with = "serialize_u32_hex",
    //     deserialize_with = "deserialize_u32_hex"
    // )]
    #[serde(skip)]
    pub text_vma_pointer: u32, // Literal VMA pointer to the string
    #[serde(flatten)]
    pub relative_name_pointer: RelativePointerInfo,
}

impl StringFill for JumplistItem {
    fn convert_text(&mut self) {}

    fn get_relative_pointer(&self) -> RelativePointerInfo {
        self.relative_name_pointer
    }

    fn get_text(&'_ self) -> &'_ DialogString {
        &self.string
    }

    fn pad_text(&mut self, size: u8) {
        self.string.set_padding(size);
    }

    fn set_vma_pointer(&mut self, ptr_le: u32) {
        // if self.text_vma_pointer != ptr_le {
        //     warn!(
        //         "Got {}, expected {}",
        //         encode_hex(&ptr_le.to_le_bytes()),
        //         encode_hex(&self.text_vma_pointer.to_le_bytes())
        //     );
        // }
        self.text_vma_pointer = ptr_le;
    }
}

pub async fn parse_strings<R: AsyncBufRead + AsyncSeek + Unpin>(
    reader: &mut R,
    location: usize,
    count: usize,
    pointers: &mut Vec<u32>,
) -> Result<BTreeMap<Hexu32, JumplistItem>, io::Error> {
    reader.seek(SeekFrom::Start(location as u64)).await?;
    let mut pointer_bytes = [0u8; 4];
    let mut pointer_vec = Vec::with_capacity(count);
    let mut strings = BTreeMap::new();
    for string_no in 0..count {
        reader.read_exact(&mut pointer_bytes).await?;
        let pointer = u32::from_le_bytes(pointer_bytes);
        pointer_vec.push((string_no, pointer));
        pointers.push(pointer);
    }
    assert_eq!(
        pointer_vec.len(),
        count,
        "Number of string items must be EXACT!"
    );
    for (string_no, ptr) in pointer_vec {
        reader
            .seek(SeekFrom::Start(u64::from(ptr - POINTER_OFFSET)))
            .await?;
        let mut string_bytes = Vec::with_capacity(20);
        reader.read_until(0, &mut string_bytes).await?;
        let mut string_bytes_iter = string_bytes.into_iter();
        let mut engrish_bytes = Vec::with_capacity(128);
        while let Some(byte) = string_bytes_iter.next()
            && byte != 0
        {
            engrish_bytes.push(byte);
        }
        let engrish_str = decode_psg2_string(engrish_bytes);
        strings.insert(
            Hexu32(u32::try_from(string_no).unwrap()),
            JumplistItem {
                string: engrish_str,
                text_vma_pointer: ptr,
                relative_name_pointer: RelativePointerInfo::default(),
            },
        );
    }
    Ok(strings)
}

#[inline]
pub async fn patch_strings(
    exec_writer: &mut BufWriter<fs::File>,
    jumplist_items: BTreeMap<Hexu32, JumplistItem>,
    location: usize,
    count: usize,
) -> Result<(), io::Error> {
    exec_writer.seek(SeekFrom::Start(location as u64)).await?;
    assert_eq!(count, jumplist_items.len(), "Jumplist count MUST be exact!");
    for (_, jumplist_item) in jumplist_items {
        // Now write it all
        exec_writer
            .write_all(&jumplist_item.text_vma_pointer.to_le_bytes())
            .await?;
    }
    Ok(())
}

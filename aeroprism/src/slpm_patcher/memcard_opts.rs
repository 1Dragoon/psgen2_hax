use crate::helpers::{deserialize_u32_hex, encode_hex, serialize_u32_hex};
use log::warn;
use crate::{
    events::{
        DialogItem, DialogString, codec::decode_psg2_string, deserialize_dialog_items,
        serialize_dialog_items,
    },
    helpers::Hexu32,
    slpm_patcher::{POINTER_OFFSET, RelativePointerInfo, StringFill},
};
use alloc::collections::BTreeMap;
use serde::{Deserialize, Serialize};
use std::io;
use tokio::{
    fs,
    io::{AsyncBufRead, AsyncReadExt, AsyncSeek, AsyncSeekExt, AsyncWriteExt, BufWriter, SeekFrom},
};

static MEMCARD_STRUCT_START: usize = 0x18_E0A0;
static MEMCARD_STRUCT_COUNT: usize = 9;
static MEMCARD_STRUCT_FIELDS: usize = 2;

#[expect(
    clippy::arbitrary_source_item_ordering,
    reason = "Ordered by binary struct fields."
)]
#[derive(Serialize, Deserialize)]
pub struct MemcardOpt {
    #[serde(
        deserialize_with = "deserialize_dialog_items",
        serialize_with = "serialize_dialog_items"
    )]
    pub string: Vec<DialogItem>,
    #[serde(
        serialize_with = "serialize_u32_hex",
        deserialize_with = "deserialize_u32_hex"
    )]
    // #[serde(skip)]
    pub name_vma_pointer: u32,
    #[serde(skip)]
    pub text: DialogString,
    #[serde(flatten)]
    pub relative_name_pointer: RelativePointerInfo,
    pub unknown_1: i32,
}

impl StringFill for MemcardOpt {
    fn convert_text(&mut self) {
        self.text = DialogString {
            text: self.string.clone(),
            padding: 0,
        };
    }

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
                encode_hex(&ptr_le.to_le_bytes()),
                encode_hex(&self.name_vma_pointer.to_le_bytes())
            );
        }
        self.name_vma_pointer = ptr_le;
    }
}

pub async fn parse<R: AsyncBufRead + AsyncSeek + Unpin>(
    reader: &mut R,
    pointers: &mut Vec<u32>,
) -> Result<BTreeMap<Hexu32, MemcardOpt>, io::Error> {
    reader
        .seek(SeekFrom::Start(MEMCARD_STRUCT_START as u64))
        .await?;
    let mut field_bytes = [0u8; 4];
    let mut memcard_opts: BTreeMap<Hexu32, MemcardOpt> = BTreeMap::new();
    for mc_opt_no in 0..MEMCARD_STRUCT_COUNT {
        let mut fields = Vec::with_capacity(MEMCARD_STRUCT_FIELDS);
        for _ in 0..MEMCARD_STRUCT_FIELDS {
            reader.read_exact(&mut field_bytes).await?;
            fields.push(field_bytes);
        }
        let pointer_bytes = fields.pop().unwrap();
        let name_vma_pointer = u32::from_le_bytes(pointer_bytes);
        pointers.push(name_vma_pointer);
        let memcard_opt = MemcardOpt {
            string: Vec::new(),
            text: DialogString::default(),
            name_vma_pointer,
            relative_name_pointer: RelativePointerInfo::default(),
            unknown_1: i32::from_le_bytes(fields.pop().unwrap()),
        };
        memcard_opts.insert(Hexu32(u32::try_from(mc_opt_no).unwrap()), memcard_opt);
    }

    // let mut name_pointers = BTreeMap::new();

    for memcard_opt in memcard_opts.values_mut() {
        let ptr = memcard_opt.name_vma_pointer;
        reader
            .seek(SeekFrom::Start(u64::from(ptr - POINTER_OFFSET)))
            .await?;
        let mut string_bytes = Vec::with_capacity(20);
        while let Ok(byte) = reader.read_u8().await
            && byte != 0
        {
            string_bytes.push(byte);
        }
        memcard_opt.string = decode_psg2_string(string_bytes).text;
        // name_pointers.insert(
        //     Hexu32(memcard_opt.name_pointer - 0xff000),
        //     (
        //         crate::helpers::encode_hex(&memcard_opt.name_pointer.to_le_bytes()),
        //         memcard_opt.name.to_string(),
        //     ),
        // );
    }
    // let bytes = serde_json::to_string_pretty(&name_pointers)
    //     .unwrap()
    //     .into_bytes();
    // save_binary_file(&PathBuf::from("jap_mc_pointers.json"), &bytes).await?;

    Ok(memcard_opts)
}

#[inline]
pub async fn patch(
    exec_writer: &mut BufWriter<fs::File>,
    memcard_opts: BTreeMap<Hexu32, MemcardOpt>,
) -> Result<(), io::Error> {
    exec_writer
        .seek(SeekFrom::Start(MEMCARD_STRUCT_START as u64))
        .await?;
    assert_eq!(
        MEMCARD_STRUCT_COUNT,
        memcard_opts.len(),
        "Music count MUST be exact!"
    );
    for (_, memcard_opt) in memcard_opts {
        // Field comes before name pointer here
        exec_writer
            .write_all(&memcard_opt.unknown_1.to_le_bytes())
            .await?;
        exec_writer
            .write_all(&memcard_opt.name_vma_pointer.to_le_bytes())
            .await?;
    }
    Ok(())
}

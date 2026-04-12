use crate::helpers::{deserialize_u32_hex, encode_hex, serialize_u32_hex};
use log::warn;
use crate::{
    events::{
        DialogItem, DialogString, codec::decode_psg2_string, deserialize_dialog_items,
        serialize_dialog_items,
    },
    slpm_patcher::{Hexu32, POINTER_OFFSET, RelativePointerInfo, StringFill},
};
use alloc::collections::BTreeMap;
use serde::{Deserialize, Serialize};
use std::io;
use tokio::{
    fs,
    io::{AsyncBufRead, AsyncReadExt, AsyncSeek, AsyncSeekExt, AsyncWriteExt, BufWriter, SeekFrom},
};

static MUSIC_STRUCT_START: usize = 0x18_9450;
static MUSIC_STRUCT_COUNT: usize = 19;
static MUSIC_STRUCT_FIELDS: usize = 2;

#[expect(
    clippy::arbitrary_source_item_ordering,
    reason = "Ordered by binary struct fields."
)]
#[derive(Serialize, Deserialize)]
pub struct Song {
    #[serde(
        deserialize_with = "deserialize_dialog_items",
        serialize_with = "serialize_dialog_items"
    )]
    pub name: Vec<DialogItem>,
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

impl StringFill for Song {
    fn convert_text(&mut self) {
        self.text = DialogString {
            text: self.name.clone(),
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
) -> Result<BTreeMap<Hexu32, Song>, io::Error> {
    reader
        .seek(SeekFrom::Start(MUSIC_STRUCT_START as u64))
        .await?;
    let mut field_bytes = [0u8; 4];
    let mut songs = BTreeMap::new();
    for song_no in 0..MUSIC_STRUCT_COUNT {
        let mut fields = Vec::with_capacity(MUSIC_STRUCT_FIELDS);
        for _ in 0..MUSIC_STRUCT_FIELDS {
            reader.read_exact(&mut field_bytes).await?;
            fields.push(field_bytes);
        }
        let pointer_bytes = fields.pop().unwrap();
        let name_vma_pointer = u32::from_le_bytes(pointer_bytes);
        pointers.push(name_vma_pointer);
        let song = Song {
            name: Vec::new(),
            name_vma_pointer,
            text: DialogString::default(),
            relative_name_pointer: RelativePointerInfo::default(),
            unknown_1: i32::from_le_bytes(fields.pop().unwrap()),
        };
        songs.insert(Hexu32(u32::try_from(song_no).unwrap()), song);
    }

    // let mut name_pointers = BTreeMap::new();

    for song in songs.values_mut() {
        let ptr = song.name_vma_pointer;
        reader
            .seek(SeekFrom::Start(u64::from(ptr - POINTER_OFFSET)))
            .await?;
        let mut string_bytes = Vec::with_capacity(20);
        while let Ok(byte) = reader.read_u8().await
            && byte != 0
        {
            string_bytes.push(byte);
        }
        song.name = decode_psg2_string(string_bytes).text;
        // name_pointers.insert(
        //     Hexu32(song.name_pointer - 0xff000),
        //     (
        //         crate::helpers::encode_hex(&song.name_pointer.to_le_bytes()),
        //         song.name.to_string(),
        //     ),
        // );
    }
    // let bytes = serde_json::to_string_pretty(&name_pointers)
    //     .unwrap()
    //     .into_bytes();
    // save_binary_file(&PathBuf::from("jap_song_pointers.json"), &bytes).await?;

    Ok(songs)
}

#[inline]
pub async fn patch(
    exec_writer: &mut BufWriter<fs::File>,
    songs: BTreeMap<Hexu32, Song>,
) -> Result<(), io::Error> {
    exec_writer
        .seek(SeekFrom::Start(MUSIC_STRUCT_START as u64))
        .await?;
    assert_eq!(
        MUSIC_STRUCT_COUNT,
        songs.len(),
        "Music count MUST be exact!"
    );
    for (_, song) in songs {
        // Field comes before name pointer here
        exec_writer.write_all(&song.unknown_1.to_le_bytes()).await?;
        exec_writer
            .write_all(&song.name_vma_pointer.to_le_bytes())
            .await?;
    }
    Ok(())
}

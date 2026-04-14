// use crate::{
//     events::{IndexMapWrapper, codec::parse_events, rebuild_event},
//     helpers::hex_edit_encode,
//     lz77_le::{compress_lz77_le, decompress},
//     save_binary_file,
//     sggg_codec::{convert_to_png, png_to_sggg},
// };
// use alloc::{collections::BTreeMap, sync::Arc};
// use core::time::Duration;
// use log::{Level, debug, info, log_enabled, trace, warn};
// use std::{
//     ffi::{OsStr, OsString},
//     io::{Cursor, ErrorKind, Read},
//     path::{Path, PathBuf},
// };
// use std::io::{self};

// pub fn read_sdat(data: Vec<u8>) -> Result<Vec<u8>, io::Error> {
//     let mut blob_reader = Cursor::new(data);
//     let mut field = [0u8; 4];
//     let mut chunk_sizes = Vec::with_capacity(40);
//     while field != [0u8; 4] {
//         blob_reader.read_exact(&mut field)?;
//         chunk_sizes.push(u32::from_le_bytes(field));
//     }
//     todo!()
// }
use core::{convert, error, fmt, num::ParseIntError};
use std::path::Path;
use tokio::{fs, io};

const HEX_BYTES: &str = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f\
                         202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f\
                         404142434445464748494a4b4c4d4e4f505152535455565758595a5b5c5d5e5f\
                         606162636465666768696a6b6c6d6e6f707172737475767778797a7b7c7d7e7f\
                         808182838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9f\
                         a0a1a2a3a4a5a6a7a8a9aaabacadaeafb0b1b2b3b4b5b6b7b8b9babbbcbdbebf\
                         c0c1c2c3c4c5c6c7c8c9cacbcccdcecfd0d1d2d3d4d5d6d7d8d9dadbdcdddedf\
                         e0e1e2e3e4e5e6e7e8e9eaebecedeeeff0f1f2f3f4f5f6f7f8f9fafbfcfdfeff";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeHexError {
    OddLength,
    ParseInt(ParseIntError),
}

impl From<ParseIntError> for DecodeHexError {
    fn from(e: ParseIntError) -> Self {
        Self::ParseInt(e)
    }
}

impl fmt::Display for DecodeHexError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::OddLength => "input string has an odd number of bytes".fmt(f),
            Self::ParseInt(e) => e.fmt(f),
        }
    }
}

impl error::Error for DecodeHexError {}

#[expect(clippy::string_slice, reason = "Hex strings are all ascii")]
pub fn decode_hex(s: &str) -> Result<Vec<u8>, DecodeHexError> {
    if s.len().is_multiple_of(2) {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(convert::Into::into))
            .collect()
    } else {
        Err(DecodeHexError::OddLength)
    }
}

pub fn encode_hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|&b|
            // SAFETY:
            // Every hex value covered
            unsafe {
                let i = 2 * b as usize;
                HEX_BYTES.get_unchecked(i..i + 2)
            })
        .collect()
}

// Output hex in a format similar to a hex editor
pub fn hex_edit_encode(bytes: &[u8]) -> String {
    bytes
        .iter()
        .enumerate()
        .map(|(pos, &b)| {
            let i = 2 * b as usize;
            if (pos + 1) % 32 == 0 {
                // SAFETY:
                // Every hex value covered
                format!("{}\n", unsafe { HEX_BYTES.get_unchecked(i..i + 2) })
            } else if (pos + 1) % 4 == 0 {
                // SAFETY:
                // Every hex value covered
                format!("{} ", unsafe { HEX_BYTES.get_unchecked(i..i + 2) })
            } else {
                // SAFETY:
                // Every hex value covered
                unsafe { HEX_BYTES.get_unchecked(i..i + 2) }.into()
            }
        })
        .collect()
}

pub async fn copy_dir_all<P: AsRef<Path> + Sync + Send>(src: P, dst: P) -> io::Result<()> {
    fs::create_dir_all(&dst).await?;
    let mut read_dir = fs::read_dir(&src).await.unwrap();
    while let Some(dir_entry) = read_dir.next_entry().await.unwrap() {
        let ty = dir_entry.file_type().await?;
        if ty.is_dir() {
            Box::pin(copy_dir_all(
                dir_entry.path(),
                dst.as_ref().join(dir_entry.file_name()),
            ))
            .await?;
        } else {
            let dest = dst.as_ref().join(dir_entry.file_name());
            #[cfg(target_os = "windows")]
            if dest.exists() {
                use std::fs::set_permissions;
                let mut perms = fs::metadata(&dest).await?.permissions();
                if perms.readonly() {
                    #[expect(
                        clippy::permissions_set_readonly_false,
                        reason = "lint is only relevant to non-windows systems"
                    )]
                    perms.set_readonly(false);
                    set_permissions(&dest, perms)?;
                }
            }
            fs::copy(dir_entry.path(), dest).await?;
        }
    }
    Ok(())
}

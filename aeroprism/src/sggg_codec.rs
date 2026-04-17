#![expect(clippy::single_call_fn, reason = "readability")]
use crate::helpers::{decode_hex, encode_hex, save_binary_file};
use byteorder::ReadBytesExt;
use itertools::Itertools;
use log::{info, warn};
use png::{BitDepth, ColorType, Compression, InterlaceInfo};
use std::{
    collections::HashSet,
    io::{self, BufRead, Cursor, Seek},
    path::{Path, PathBuf},
};

const CHANNELS_PER_COLOR: usize = 4; // Each palette color is 32-bits AGBR little endian, which translates to RGBA in big endian.
// Each palette color is represented by one byte, and it's in this order
const RED_CHANNEL: usize = 0; // Red channel number
const GREEN_CHANNEL: usize = 1; // Green channel number
const BLUE_CHANNEL: usize = 2; // Blue channel number
const ALPHA_CHANNEL: usize = 3; // Alpha channel number
const PALETTE_COLOR_COUNT: usize = 256; // The palette contains 256 color entries total
const SGGG_HEADER_SIZE: usize = 16;

enum ImageData {
    Font([Box<[u8]>; 4]),
    Sprites(Box<[Box<[u8]>]>, Box<[u8]>),
}

struct SegagagaMetadata {
    alpha_bits: [u8; 256],
    color_type: ColorType,
    height: u32,
    unknown_data: u32,
    width: u32,
}

struct PngMetadata {
    height: u32,
    unknown_data: u32,
    width: u32,
}

#[inline]
pub fn sggg_to_png<R: BufRead + Seek>(
    reader: &mut R,
    prealloc_size: usize,
) -> Result<Vec<Vec<u8>>, io::Error> {
    let (sggg, image_data) = decode_sggg(reader)?;
    let mut png_images = Vec::with_capacity(3);

    // From here, let's just let the png encoder library do most of the heavy lifting...
    match image_data {
        ImageData::Font(pixel_data) => {
            for pixels in pixel_data {
                png_images.push(encode_png(prealloc_size, &sggg, &[], &pixels)?);
            }
        }
        ImageData::Sprites(palettes, pixel_data) => {
            png_images.push(encode_png(prealloc_size, &sggg, &palettes, &pixel_data)?);
        }
    }
    png_images.shrink_to_fit();
    Ok(png_images)
}

#[inline]
fn decode_sggg<R: BufRead + Seek>(
    reader: &mut R,
) -> Result<(SegagagaMetadata, ImageData), io::Error> {
    let (width, height, unknown_data) = parse_sggg_header(reader)?;
    let sggg_palette = read_sggg_palette(reader)?;
    let mut unique_colors = HashSet::with_capacity(256);
    for color in &sggg_palette {
        unique_colors.insert([
            color[RED_CHANNEL],
            color[GREEN_CHANNEL],
            color[BLUE_CHANNEL],
        ]);
    }
    let color_type = if unique_colors.len() == 1 {
        ColorType::Grayscale
    } else {
        ColorType::Indexed
    };

    // Generate an alpha palette that has a first element as zero, followed by fully opaque 0xFF for everything else.
    let mut alpha_bits = [0xFF; PALETTE_COLOR_COUNT];
    alpha_bits[0] = 0;
    let pixels = flatten_sggg_scanlines(reader, width, height)?;

    let image_data = if color_type == ColorType::Grayscale {
        // This is the font file. The pixel map is really three (up to four) 2-bpp images encoded into a single image by superimposing them.
        ImageData::Font(split_font_planes(&pixels))
    } else {
        let mut palettes = Vec::with_capacity(4);
        palettes.push(palette_sggg_to_rgb(sggg_palette));
        // Gather additional palettes, if present
        while let Ok(additional_sggg_palette) = read_sggg_palette(reader) {
            let png_palette = palette_sggg_to_rgb(additional_sggg_palette);
            palettes.push(png_palette);
        }
        palettes.shrink_to_fit();
        ImageData::Sprites(palettes.into_boxed_slice(), pixels)
    };

    let sggg = SegagagaMetadata {
        alpha_bits,
        color_type,
        height,
        unknown_data,
        width,
    };

    Ok((sggg, image_data))
}

#[inline]
fn encode_sggg(image_data: ImageData, png_data: &PngMetadata) -> Vec<u8> {
    let mut sggg = Vec::with_capacity(
        SGGG_HEADER_SIZE
            + (PALETTE_COLOR_COUNT * CHANNELS_PER_COLOR)
            + (png_data.width * png_data.height) as usize,
    );

    // First build the header
    sggg.extend(*b"SGGG");
    sggg.extend([1, 0, 0, 0]);
    let width_u16: u16 = png_data.width.try_into().unwrap();
    let height_u16: u16 = png_data.height.try_into().unwrap();
    sggg.extend(width_u16.to_le_bytes());
    sggg.extend(height_u16.to_le_bytes());
    // Now for that unknown fourth field...
    sggg.extend(png_data.unknown_data.to_le_bytes());

    match image_data {
        ImageData::Font(pixel_planes) => {
            sggg.extend(generate_font_palette());
            sggg.extend(superimpose_font_planes(&pixel_planes));
        }
        ImageData::Sprites(palettes, pixels) => {
            let mut palette_iter = palettes.into_iter();
            let main_palette = palette_iter.next().unwrap();
            // Add the main palette
            sggg.extend(palette_rgb_to_sggg(&main_palette));

            // Add the pixels, splitting from widths higher than 512 if necessary
            let width_usize = usize::try_from(png_data.width).unwrap();
            let height_usize = usize::try_from(png_data.height).unwrap();
            if width_usize > 512 {
                let lines = pixels.into_vec().into_iter().chunks(width_usize);
                let mut base_lines = Vec::with_capacity(height_usize);
                let mut extended_lines = Vec::with_capacity(height_usize);
                for line in &lines {
                    let mut line_vec = line.into_iter().collect::<Vec<_>>();
                    base_lines.extend(line_vec.drain(0..512));
                    extended_lines.extend(line_vec);
                }
                sggg.extend(base_lines);
                sggg.extend(extended_lines);
            } else {
                sggg.extend(pixels);
            }

            // Add the extra palettes, if present
            for palette in palette_iter {
                sggg.extend(palette_rgb_to_sggg(&palette));
            }
        }
    }
    sggg
}

#[inline]
fn decode_png<R: BufRead + Seek>(reader: &mut R) -> Result<(ImageData, PngMetadata), String> {
    let mut png_reader = png::Decoder::new(reader)
        .read_info()
        .map_err(|e| format!("Error reading PNG info: {e}"))?;
    let info = png_reader.info();
    match info.bit_depth {
        BitDepth::Eight | BitDepth::Two => {
            // esta bien
        }
        other => {
            return Err(format!(
                "PNG must be either 8-bit or 2-bit color depth. Got {other:?}"
            ));
        }
    }
    let width = info.width;
    if width > 1024 {
        warn!(
            "Pixel widths greater than 1024 are not supported. It's unknown how SGGG stores widths greater than this. Anything we do is just a guess."
        );
    }
    let height = info.height;
    let mut unknown_field = [0; 4];
    let mut from_png_palette_hash = [0; 128];
    for ttxt_chunk in &info.uncompressed_latin1_text {
        match ttxt_chunk.keyword.as_str() {
            "Header4" => {
                let bytes = decode_hex(&ttxt_chunk.text)
                    .map_err(|e| format!("Error decoding Header4 hex value: {e}"))?;
                if bytes.len() > 4 {
                    return Err(format!(
                        "Header4 value is too long! Contents: {}",
                        ttxt_chunk.text
                    ));
                }
                #[expect(clippy::indexing_slicing, reason = "the range is checked already")]
                for (i, byte) in bytes.into_iter().enumerate() {
                    unknown_field[i] = byte;
                }
            }
            "PaletteMeowhash" => {
                let bytes = decode_hex(&ttxt_chunk.text)
                    .map_err(|e| format!("Error decoding PaletteMeowhash hex value: {e}"))?;
                if bytes.len() > 128 {
                    return Err(format!(
                        "PaletteMeowhash value is too long! Contents: {}",
                        ttxt_chunk.text
                    ));
                }
                #[expect(clippy::indexing_slicing, reason = "the range is checked already")]
                for (i, byte) in bytes.into_iter().enumerate() {
                    from_png_palette_hash[i] = byte;
                }
            }
            _ => {
                // no action needed
            }
        }
    }
    let mut ztext_data = Vec::with_capacity(info.compressed_latin1_text.len());
    for txt in &info.compressed_latin1_text {
        ztext_data.push((txt.keyword.clone(), txt.get_text().unwrap()));
    }
    let color_type = info.color_type;
    let mut row_num = 0;
    let pixel_row = &mut vec![0; width.try_into().unwrap()];
    let mut pixels = Vec::with_capacity(usize::try_from(width * height * 3).unwrap());
    while let Some(interlace_info) = png_reader
        .read_row(pixel_row)
        .map_err(|e| format!("Error reading PNG row {row_num}: {e}"))?
    {
        #[expect(
            clippy::match_wildcard_for_single_variants,
            reason = "blanket check for interlacing, no intention of ever adding support for it"
        )]
        match interlace_info {
            InterlaceInfo::Null(_) => {
                // esta bien
            }
            _ => {
                warn!(
                    "Interlacing detected on PNG row {row_num}. This isn't supported and may cause anomalous behavior."
                );
            }
        }
        pixels.extend(pixel_row.iter());
        row_num += 1;
    }
    let image_data = match color_type {
        ColorType::Grayscale => ImageData::Font([
            pixels.into_boxed_slice(),
            Box::new([]),
            Box::new([]),
            Box::new([]),
        ]),
        ColorType::Indexed => {
            let mut palettes = Vec::with_capacity(ztext_data.len() + 1);
            let palette = png_reader.info().palette.as_deref().ok_or_else(|| "Indexed PNG is missing its PLTE (palette) chunk. That breaks the spec and we can't rebuild the SGGG palette without it.".to_owned())?.to_vec();
            // Check if the palette hash is set
            if from_png_palette_hash.iter().all(|b| *b == 0) {
                info!("Palette hash wasn't stored; can't verify whether the palette is untouched.");
            } else {
                let palette_hash = meowhash::MeowHasher::hash(&palette);
                let stored_hash = meowhash::MeowHash::from_bytes(from_png_palette_hash);
                if palette_hash != stored_hash {
                    warn!(
                        "Palette hash mismatch from the original SGGG. This may cause anomalous behavior. Please ensure your image editor preserves the original palette."
                    );
                }
            }
            palettes.push(palette.into_boxed_slice());
            ztext_data.sort_unstable_by(|(key_a, _), (key_b, _)| key_a.cmp(key_b));
            for (k, v) in ztext_data {
                if k.starts_with("AltPalette") {
                    let alt_palette = decode_hex(&v).unwrap();
                    // let mut sggg_palette = palette_rgb_to_sggg(&alt_palette);
                    palettes.push(alt_palette.into_boxed_slice());
                }
            }

            ImageData::Sprites(palettes.into_boxed_slice(), pixels.into_boxed_slice())
            // palette_rgb_to_sggg(plte_data)
        }
        other => {
            return Err(format!(
                "Color type must be either grayscale (type 0) or indexed (type 3, aka paletted). Got: {other:?}"
            ));
        }
    };
    let png_data = PngMetadata {
        height,
        unknown_data: u32::from_le_bytes(unknown_field),
        width,
    };
    Ok((image_data, png_data))
}

#[inline]
fn encode_png(
    prealloc_size: usize,
    sggg: &SegagagaMetadata,
    palettes: &[Box<[u8]>],
    pixel_data: &[u8],
) -> Result<Vec<u8>, io::Error> {
    let mut writer = Cursor::new(vec![0; prealloc_size]);
    let mut png_encoder = png::Encoder::new(&mut writer, sggg.width, sggg.height);
    if matches!(sggg.color_type, ColorType::Indexed) {
        // This is a 32-bit paletted color image. Each 1 byte pixel just points to a palette offset. Thus in PNG speak, this is called indexed color
        let palette = palettes.first().unwrap();
        let palette_hash = meowhash::MeowHasher::hash(palette).into_bytes();
        png_encoder.add_text_chunk("PaletteMeowhash".into(), encode_hex(&palette_hash))?;
        png_encoder.set_palette(&**palette);
        png_encoder.set_trns(&sggg.alpha_bits);
        png_encoder.set_depth(BitDepth::Eight);
        // Store the additional palettes as compressed text chunks, if there is more than one
        for (i, png_palette) in palettes.iter().skip(1).enumerate() {
            png_encoder.add_ztxt_chunk(format!("AltPalette{}", i + 1), encode_hex(png_palette))?;
        }
    } else {
        png_encoder.set_depth(BitDepth::Two);
    }
    png_encoder.set_color(sggg.color_type);
    png_encoder.set_compression(Compression::NoCompression);
    if sggg.unknown_data > 0 {
        png_encoder.add_text_chunk(
            "Header4".into(),
            encode_hex(&sggg.unknown_data.to_le_bytes()),
        )?;
    }
    let mut png_writer = png_encoder.write_header()?;
    png_writer.write_image_data(pixel_data)?;
    png_writer.finish()?;
    let mut png_data = writer.into_inner();
    png_data.shrink_to_fit();
    Ok(png_data)
}

#[inline]
fn superimpose_font_planes(pixel_planes: &[Box<[u8]>; 4]) -> Vec<u8> {
    let [image_a, image_b, image_c, image_d] = pixel_planes;
    let array_size = image_a.len();
    let mut pixels = Vec::with_capacity(array_size);
    let mask = 0x03;
    #[expect(clippy::indexing_slicing, reason = "more concise to copy bits")]
    for i in 0..image_a.len() {
        let mut byte = 0u8;
        byte |= image_a[i] & mask;
        byte |= (image_b[i] << 2) & (mask << 2);
        byte |= (image_c[i] << 4) & (mask << 4);
        byte |= (image_d[i] << 6) & (mask << 6);
        pixels.push(byte);
    }
    pixels
}

fn split_font_planes(pixels: &[u8]) -> [Box<[u8]>; 4] {
    let mut image_a = Vec::with_capacity(pixels.len() / 3);
    let mut image_b = Vec::with_capacity(pixels.len() / 3);
    let mut image_c = Vec::with_capacity(pixels.len() / 3);
    let mut image_d = Vec::with_capacity(pixels.len() / 3);
    for bytes in pixels.chunks(4) {
        let mut a = 0u8;
        let mut b = 0u8;
        let mut c = 0u8;
        let mut d = 0u8;
        let mask = 0b0000_0011;
        for byte in bytes {
            a <<= 2;
            a |= byte & mask;
        }
        image_a.push(a);
        for byte in bytes {
            b <<= 2;
            b |= (byte >> 2) & mask;
        }
        image_b.push(b);
        for byte in bytes {
            c <<= 2;
            c |= (byte >> 4) & mask;
        }
        image_c.push(c);
        for byte in bytes {
            d <<= 2;
            d |= (byte >> 6) & mask;
        }
        image_d.push(d);
    }
    [
        image_a.into_boxed_slice(),
        image_b.into_boxed_slice(),
        image_c.into_boxed_slice(),
        image_d.into_boxed_slice(),
    ]
}

#[inline]
pub fn png_to_sggg<R: BufRead + Seek>(reader: &mut R) -> Result<Vec<u8>, String> {
    let (image_data, png_data) = decode_png(reader)?;

    // Now to build the SGGG file
    let sggg = encode_sggg(image_data, &png_data);
    Ok(sggg)
}

#[inline]
fn generate_font_palette() -> Vec<u8> {
    [0x00, 0x3a, 0x5f, 0x7f]
        .iter()
        .cycle()
        .take(PALETTE_COLOR_COUNT)
        .flat_map(|alpha_byte| [0xFF, 0xFF, 0xFF, *alpha_byte])
        .collect::<Vec<_>>()
}

#[inline]
fn parse_sggg_header<R: BufRead + Seek>(reader: &mut R) -> Result<(u32, u32, u32), io::Error> {
    let current_u16 = &mut [0u8; 2];
    let current_u32 = &mut [0u8; 4];
    // Toss the magic bytes
    reader.read_exact(current_u32)?;
    // And toss the "version"
    reader.read_exact(current_u32)?;

    reader.read_exact(current_u16)?;
    let width = u32::from(u16::from_le_bytes(*current_u16));
    reader.read_exact(current_u16)?;
    let height = u32::from(u16::from_le_bytes(*current_u16));

    // No idea what this field is for, but just to maintain fidelity, we'll preserve it in a PNG text chunk for later reconstitution.
    reader.read_exact(current_u32)?;
    let unknown_data = u32::from_le_bytes(*current_u32);
    Ok((width, height, unknown_data))
}

#[inline]
fn read_sggg_palette<R: BufRead + Seek>(reader: &mut R) -> Result<Vec<[u8; 4]>, io::Error> {
    let current_u32 = &mut [0u8; 4];
    let mut palette: Vec<[u8; CHANNELS_PER_COLOR]> =
        Vec::with_capacity(PALETTE_COLOR_COUNT * CHANNELS_PER_COLOR);
    for _ in 0..PALETTE_COLOR_COUNT {
        reader.read_exact(current_u32)?;
        // So here's a funny thing...
        // As you know, little endian to big endian goes ABCD -> DCBA
        // Which means ABGR little endian is EXACTLY equivalent to big endian RGBA
        // Coincidentally, RGBA is the EXACT color sequence PNG uses
        // Which means if we simply copy without doing any transform...
        palette.push(*current_u32);
        // Then we get exactly what we need! How easy is that?
        // Well, almost. There's still some twiddling we need to do, but we'll do that later.
    }
    Ok(palette)
}

#[inline]
fn flatten_sggg_scanlines<R: BufRead + Seek>(
    reader: &mut R,
    width: u32,
    height: u32,
) -> Result<Box<[u8]>, io::Error> {
    // So...technically the maximum width of the sggg format is 512 pixels.
    // To work around that, anything to the right of the 512th pixel on each row gets stored after the 512*height pixel
    // So we have to noodle with the pixels a bit.
    let virtual_width = 512.min(width);
    let mut pixel_rows = Vec::with_capacity(height as usize);
    for _ in 0..height {
        let mut pixel_row = Vec::with_capacity(width as usize);
        for _ in 0..virtual_width {
            pixel_row.push(reader.read_u8()?);
        }
        pixel_rows.push(pixel_row);
    }
    // And for the remainder, let's get each one and append it to the column it belongs to
    if width > 512 {
        for i in 0..height {
            #[expect(clippy::indexing_slicing, reason = "the range is checked already")]
            for _ in 0..(width - virtual_width) {
                pixel_rows[i as usize].push(reader.read_u8()?);
            }
        }
    }
    Ok(pixel_rows
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .into_boxed_slice())
}

#[inline]
fn palette_sggg_to_rgb(mut palette: Vec<[u8; 4]>) -> Box<[u8]> {
    // Prepare an SGGG palette for use in a PNG
    twiddle_palette(&mut palette);
    palette
        .iter()
        .flat_map(|color| {
            [
                color[RED_CHANNEL],
                color[GREEN_CHANNEL],
                color[BLUE_CHANNEL],
            ]
        })
        .collect::<Vec<_>>()
        .into_boxed_slice()
}

#[inline]
#[expect(clippy::indexing_slicing, reason = "Readability")]
fn palette_rgb_to_sggg(rgb_palette: &[u8]) -> Vec<u8> {
    let mut sggg_palette = Vec::with_capacity(PALETTE_COLOR_COUNT);
    rgb_palette.chunks_exact(3).for_each(|chunk| {
        sggg_palette.push([
            chunk[RED_CHANNEL],
            chunk[GREEN_CHANNEL],
            chunk[BLUE_CHANNEL],
            0x80,
        ]);
    });
    // Set the first pixel to have fully transparent alpha
    sggg_palette[0][ALPHA_CHANNEL] = 0;
    // Restore the expected SGGG palette color order
    twiddle_palette(&mut sggg_palette);
    sggg_palette.into_flattened()
}

#[inline]
#[expect(clippy::indexing_slicing, reason = "far more concise")]
fn twiddle_palette(palette: &mut Vec<[u8; 4]>) {
    // What is probably due to the way the PS2 GPU renders graphics, for every 32 colors (128 bytes) in the color palette, we have to flip the middle 16 color sets
    palette
        .as_mut_slice()
        .chunks_exact_mut(32)
        .for_each(|chunks| {
            assert!(chunks.len() > 23, "Not enough bytes to twiddle");
            let b0 = chunks[8];
            let b1 = chunks[9];
            let b2 = chunks[10];
            let b3 = chunks[11];
            let b4 = chunks[12];
            let b5 = chunks[13];
            let b6 = chunks[14];
            let b7 = chunks[15];
            chunks[8] = chunks[16]; // c0
            chunks[9] = chunks[17]; // c1
            chunks[10] = chunks[18]; // c2
            chunks[11] = chunks[19]; // c3
            chunks[12] = chunks[20]; // c4
            chunks[13] = chunks[21]; // c5
            chunks[14] = chunks[22]; // c6
            chunks[15] = chunks[23]; // c7
            chunks[16] = b0;
            chunks[17] = b1;
            chunks[18] = b2;
            chunks[19] = b3;
            chunks[20] = b4;
            chunks[21] = b5;
            chunks[22] = b6;
            chunks[23] = b7;
        });
}

#[inline]
pub async fn convert_to_png(
    save_path: &Path,
    stem_name: &str,
    extensions: &Vec<&str>,
    data: &Vec<u8>,
) -> Result<(), io::Error> {
    // Reference? https://en.wikipedia.org/wiki/Segagaga
    // This file format seems most appropriate as a png rather than bmp.
    // Harder to screw up, readily translates, has an alpha channel, can store extra data that we need
    let sggg_size = data.len();
    let sggg_reader = &mut Cursor::new(data);
    let mut png_files = sggg_to_png(sggg_reader, sggg_size)?;

    if png_files.len() == 1 {
        let png_file = png_files.pop().unwrap();
        let leaf_name = format!("{stem_name}.{}", extensions.join("."));
        let main_save_path = save_path.join(leaf_name);
        save_binary_file(&main_save_path, &png_file).await?;
    } else {
        for (i, png_file) in png_files.into_iter().enumerate() {
            let leaf_name = format!("{stem_name}-{}.{}", i + 1, extensions.join("."));
            let main_save_path = save_path.join(leaf_name);
            save_binary_file(&main_save_path, &png_file).await?;
        }
    }

    Ok(())
}

// Use for unit tests
// let reconstituted_data = png_to_sggg(&mut Cursor::new(&*pngdata))?;
// for i in 0..reconstituted_data.len() {
//     let j = i.saturating_sub(8);
//     if reconstituted_data[i] != data[i] {
//         panic!(
//             "SGGG {stem_name} Beginning at {j}\nGot: \n{}\nExpected:\n{}",
//             encode_hex(&reconstituted_data[j..i + 8]),
//             encode_hex(&data[j..i + 8])
//         )
//     }
// }
// if reconstituted_data.len() != data.len() {
//     println!(
//         "Warning: SGGG {stem_name} Reconstituted data is {} bytes, original is {} bytes",
//         reconstituted_data.len(),
//         data.len()
//     )
// }

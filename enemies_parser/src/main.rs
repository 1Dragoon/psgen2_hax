use enemydata::{enemy_name_bytes, enemy_struct_bytes};
use std::{
    borrow::Cow,
    fs::File,
    io::{BufWriter, Write},
};
mod enemydata;
use clap::Parser;
use std::path::PathBuf;

const STRING_PTR_OFFSET: u32 = 0x2A79E0;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    /// Where to dump the parsed data
    #[arg(short, long, default_value = "../psgen2_enemies.md")]
    file: PathBuf,
}

fn main() {
    let cli = Cli::parse();

    let f = File::create(&cli.file)
        .unwrap_or_else(|_| panic!("Unable to open {}", cli.file.to_string_lossy()));
    let mut file = BufWriter::new(f);
    let enemy_data_structs = enemy_struct_bytes().chunks(148).collect::<Vec<_>>();
    for (enemy_no, enemy_struct) in enemy_data_structs.iter().enumerate() {
        let struct_items = enemy_struct.chunks(4).collect::<Vec<_>>();
        for (item_no, data) in struct_items.iter().enumerate() {
            process_item(enemy_no, item_no, data, &mut file).unwrap_or_else(|_| {
                panic!("Failed while writing to {}", cli.file.to_string_lossy())
            });
        }
        file.write_all(b"\n")
            .unwrap_or_else(|_| panic!("Failed while writing to {}", cli.file.to_string_lossy()));
    }
}

fn process_item(
    enemy_no: usize,
    item_no: usize,
    data: &&[u8],
    file: &mut BufWriter<File>,
) -> Result<(), std::io::Error> {
    let bytes = [data[0], data[1], data[2], data[3]];
    let le_value = u32::from_le_bytes(bytes);
    match item_no {
        0 => {
            let string_offset = le_value - STRING_PTR_OFFSET;
            file.write_all(
                format!("#### {}: {}\n", enemy_no + 1, engrish(string_offset)).as_bytes(),
            )?;
        }
        1 => {
            let strengths = data[0] >> 4;
            let weaknesses = data[0] & 0xf;
            let mut buffer = String::with_capacity(64);
            // buffer.push_str(&hex::encode(data));
            // buffer.push(' ');
            // Resistance
            buffer.push_str("R:");
            if strengths & 0x1 == 0x1 {
                buffer.push('🔥'); // Fire
            }
            if strengths & 0x2 == 0x2 {
                buffer.push('🧊'); // Ice
            }
            if strengths & 0x4 == 0x4 {
                buffer.push('💨'); // Wind
            }
            if strengths & 0x8 == 0x8 {
                buffer.push('⚡'); // Lightning
            }
            if strengths & 0xf == 0x0 {
                buffer.push('❌');
            }
            // Weakness
            buffer.push_str("W:");
            if weaknesses & 0x1 == 0x1 {
                buffer.push('🔥'); // Fire
            }
            if weaknesses & 0x2 == 0x2 {
                buffer.push('🧊'); // Ice
            }
            if weaknesses & 0x4 == 0x4 {
                buffer.push('💨'); // Wind
            }
            if weaknesses & 0x8 == 0x8 {
                buffer.push('⚡'); // Lightning
            }
            if weaknesses & 0xf == 0x0 {
                buffer.push('❌');
            }
            // data[1] appears to be unused
            let enemy_types = data[2] >> 4;
            // Enemy types
            buffer.push_str("T:");
            if enemy_types & 0x1 == 0x1 {
                buffer.push_str("☣️"); // Biological
            }
            if enemy_types & 0x2 == 0x2 {
                buffer.push_str("⚙️"); // Machine
            }
            if enemy_types & 0x3 == 0x0 {
                buffer.push('😈'); // Demon
            }
            if enemy_types & 0x4 == 0x4 {
                buffer.push('💀'); // Boss
            }
            if enemy_types & 0x8 == 0x8 {
                buffer.push('👹') // Super boss (?)
            }
            // data[3] appears to control graphical effects, e.g. whether the enemy graphic floats around, sits still, flashes, etc
            file.write_all(format!("- {buffer}\n").as_bytes())?;
        }
        2 => {
            file.write_all(format!("- ❤️  {le_value}\n").as_bytes())?; // HP
        }
        3 => {
            file.write_all(format!("- 🗡️  {le_value}\n").as_bytes())?; // Attack
        }
        4 => {
            file.write_all(format!("- 🛡️  {le_value}\n").as_bytes())?; // Defense
        }
        5 => {
            file.write_all(format!("- 🤸  {le_value}\n").as_bytes())?; // Agility
        }
        _ => {
            // 12 through 17 appear to control the art assets used for this enemy. E.g. dropping the data in these fields from mother brain into neifirst will make neifirst look liek mother brain
            // As for the rest, I don't know what they do. Uncomment the below lines to serialize all of them anyways.
            // if le_value > 0xffff {
            //     println!("- ❓ {word_number} {}",hex::encode(data));
            // } else {
            //     println!("- ❓ {word_number} {le_value}")
            // }
        }
    };
    Ok(())
}

// My naive but simple (and probably wrong) shiftjis to ascii converter
fn engrish(offset: u32) -> String {
    let mut buffer = Vec::with_capacity(64);
    let bytes = enemy_name_bytes()
        .iter()
        .skip(offset as _)
        .take_while(|b| **b != 0)
        .collect::<Cow<'_, _>>();
    for byte in bytes.iter() {
        let char = *byte & 0x7F; // Strip off the high order bit to get the ascii equivalent
        if (0x20..=0x7E).contains(&char) {
            // Keep it intact if it looks like a printable character
            buffer.push(char)
        } else {
            // Render it as escaped hex if it doesn't
            let stringy = format!("\\x{:02x}", byte).into_bytes();
            buffer.extend(stringy)
        }
    }
    String::from_utf8_lossy(&buffer).into()
}

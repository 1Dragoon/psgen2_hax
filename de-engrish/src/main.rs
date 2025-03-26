use clap::Parser;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    /// Input hex values and print out their ascii somehwat equivalents
    input: String,
    /// Or input plain text above and we'll spit out the engrished version in hex
    #[arg(short, long)]
    re_engrish: bool,
}

fn main() {
    let cli = Cli::parse();

    if cli.re_engrish {
        println!("{}", re_engrish(cli.input));
    } else {
        let bytes = hex::decode(cli.input.replace(' ', ""))
            .unwrap_or_else(|err| panic!("Error decoding hex: {err}"));
        println!("{}", engrish(&bytes));
    }
}

fn re_engrish(string: String) -> String {
    let bytes = string.as_bytes();
    let mut stringy = Vec::with_capacity(string.len());
    for byte in bytes {
        stringy.push(byte | 0x80);
    }
    hex::encode(stringy)
}

fn engrish(bytes: &[u8]) -> String {
    let mut buffer = Vec::with_capacity(64);
    for byte in bytes {
        let char = *byte & 0x7F; // Strip off the high order bit to get the ascii equivalent
        if (0x20..=0x7E).contains(&char) {
            // Keep it intact if it looks like a printable character
            buffer.push(char)
        } else if char == 0 {
            // Treat null characters as newlines for delimiting purposes
            buffer.push(b'\n');
        } else {
            // Render it as escaped hex if it doesn't look printable
            let stringy = format!("\\x{:02x}", byte).into_bytes();
            buffer.extend(stringy)
        }
    }
    String::from_utf8_lossy(&buffer).into()
}

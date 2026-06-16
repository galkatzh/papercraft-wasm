use anyhow::Result;
use std::env;
use std::path::PathBuf;

fn main() -> Result<()> {
    println!("cargo:rerun-if-changed=build.rs");
    build_helvetica()?;
    Ok(())
}

// Metrics for well-known PDF fonts are in AFM files. `pdf_metrics.rs` includes the
// generated `helvetica_afm.rs` from OUT_DIR.
fn build_helvetica() -> Result<()> {
    use std::{
        collections::BTreeMap,
        fs::File,
        io::{BufRead, BufReader, BufWriter, Write},
    };

    let out_path = PathBuf::from(env::var("OUT_DIR")?);
    let out = File::create(out_path.join("helvetica_afm.rs"))?;
    let mut out = BufWriter::new(out);

    let mut widths = BTreeMap::<u16, u32>::new(); // Unicode to width
    let mut names = BTreeMap::<String, u16>::new(); // name to Unicode
    let mut kerns = BTreeMap::<u16, Vec<(u16, i32)>>::new(); // Second-Unicode to list of (First-Unicode, kerning)

    println!("cargo:rerun-if-changed=thirdparty/afm/names.txt");
    let char_names = File::open("thirdparty/afm/names.txt")?;
    let char_names = BufReader::new(char_names);
    for line in char_names.lines() {
        let line = line?;
        let pieces: Vec<&str> = line.split('\t').collect();
        let name = pieces[0];
        let code = u16::from_str_radix(pieces[1], 16)?;
        names.insert(name.to_owned(), code);
    }

    let afm_file = "thirdparty/afm/Helvetica.afm";
    println!("cargo:rerun-if-changed={afm_file}");
    let afm = File::open(afm_file)?;
    let afm = BufReader::new(afm);

    for line in afm.lines() {
        let line = line?;
        let pieces: Vec<&str> = line.split(';').collect();
        let words0: Vec<&str> = pieces[0].split_ascii_whitespace().collect();
        if words0.is_empty() {
            continue;
        }
        match words0[0] {
            "C" => {
                let mut width: Option<u32> = None;
                let mut name: Option<&str> = None;
                for piece in &pieces[1..] {
                    let words: Vec<&str> = piece.split_ascii_whitespace().collect();
                    if words.is_empty() {
                        continue;
                    }
                    match words[0] {
                        "WX" => {
                            width = Some(words[1].parse()?);
                        }
                        "N" => {
                            name = Some(words[1]);
                        }
                        _ => {}
                    }
                }
                if let (Some(width), Some(name)) = (width, name) {
                    let Some(&char) = names.get(name) else {
                        continue;
                    };
                    widths.insert(char, width);
                }
            }
            "KPX" => {
                let Some(&c1) = names.get(words0[1]) else {
                    continue;
                };
                let Some(&c2) = names.get(words0[2]) else {
                    continue;
                };
                let kern: i32 = words0[3].parse()?;
                kerns.entry(c2).or_default().push((c1, kern));
            }
            _ => {}
        }
    }

    // Each char maps to (width, [(previous char, kerning)]).
    writeln!(
        out,
        r"
pub struct CharInfo {{
    pub width: u32,
    pub kerns: &'static [(char, i32)],
}}
        "
    )?;
    writeln!(
        out,
        "pub static CHARS: [(char, CharInfo); {}] = [",
        widths.len(),
    )?;
    for (c, w) in widths {
        write!(out, "('\\u{{{c:x}}}', CharInfo {{ width: {w}, kerns: &[")?;
        if let Some(mut ks) = kerns.remove(&c) {
            ks.sort_by_key(|(c, _)| *c);
            for (c2, k) in ks {
                write!(out, "('\\u{{{c2:x}}}', {k}), ")?;
            }
        }
        writeln!(out, "]}}),")?;
    }
    writeln!(out, "];")?;
    Ok(())
}

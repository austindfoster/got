use anyhow::Context;
use clap::{Parser, Subcommand};
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use std::ffi::CStr;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Write};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Init,
    CatFile {
        #[clap(short = 'p')]
        pretty_print: bool,
        hash: String,
    },
    HashObject {
        #[clap(short = 'w')]
        write: bool,
        path: String,
    },
    LsTree {
        treehash: String,
    },
}

enum Kind {
    Blob,
    Commit,
    Tree,
    Tag,
}

struct Object<R> {
    kind: Kind,
    size: usize,
    reader: R,
}

impl Object<()> {
    fn read(hash: &String) -> anyhow::Result<Object<impl BufRead>> {
        let file = fs::File::open(format!(".got/objects/{}/{}", &hash[..2], &hash[2..]))
            .context("read header from .got/objects")?;
        let z = ZlibDecoder::new(file);
        let mut z = BufReader::new(z);
        let mut buf = Vec::new();
        z.read_until(0, &mut buf)
            .context("read header from .got/objects")?;
        let header = CStr::from_bytes_with_nul(&buf)
            .expect("know there is exactly one nul, and it's at the end");
        let header = header
            .to_str()
            .context(".got/objects file header isn't valid UTF-8")?;
        let Some((kind, size)) = header.split_once(' ') else {
            anyhow::bail!(".got/objects file header did not start with a known type: '{header}'");
        };
        let kind = match kind {
            "blob" => Kind::Blob,
            "commit" => Kind::Commit,
            "tree" => Kind::Tree,
            "tag" => Kind::Tag,
            _ => anyhow::bail!("we do not yet know how to print a '{kind}'"),
        };
        let size = size
            .parse::<usize>()
            .context(".got/objects file header has invalid size: {size}")?;

        Ok(Object {
            kind,
            size,
            reader: z,
        })
    }
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    match args.command {
        Command::Init => {
            fs::create_dir(".got").unwrap();
            fs::create_dir(".got/objects").unwrap();
            fs::create_dir(".got/refs").unwrap();
            fs::write(".got/HEAD", "ref: refs/heads/main\n").unwrap();
            println!("Initialized got directory");
        }
        Command::CatFile { pretty_print, hash } => {
            anyhow::ensure!(
                pretty_print,
                "mode must be given without -p, and we don't support mode"
            );
            let mut object = Object::read(&hash)?;
            let mut buf = Vec::new();
            buf.resize(object.size, 0);
            object.reader.read_exact(&mut buf[..])
                .context("read true contents of .got/objects file")?;
            let n = object.reader
                .read(&mut [0])
                .context("validate EOF in .got/object file")?;
            anyhow::ensure!(n == 0, ".got/object file had {n} trailing bytes");
            let stdout = std::io::stdout();
            let mut stdout = stdout.lock();

            match object.kind {
                Kind::Blob => stdout
                    .write_all(&buf)
                    .context("write object contents to stdout")?,
                Kind::Commit => stdout
                    .write_all(&buf)
                    .context("write object contents to stdout")?,
                Kind::Tree => stdout
                    .write_all(&buf)
                    .context("write object contents to stdout")?,
                Kind::Tag => stdout
                    .write_all(&buf)
                    .context("write object contents to stdout")?,
            }
        }
        Command::HashObject { write, path } => {
            anyhow::ensure!(write, "Only write to file is supported for now");
            let buf = fs::read(path)?;
            let mut e = ZlibEncoder::new(Vec::new(), Compression::default());
            let kind = "blob ";
            let mut header = kind.as_bytes().to_vec();
            header.append(&mut buf.len().to_string().as_bytes().to_vec());
            header.push(b'\x00');
            e.write_all(&mut header)?;
            e.write_all(&buf)?;
            let hash = e.finish()?;
            let mut printedhash = String::new();

            for byte in &hash {
                printedhash.push_str(format!("{:02x}", byte).as_str());
            }
            let hashpath = format!(".got/objects/{}/{}", &printedhash[..2], &printedhash[2..]);
            fs::create_dir_all(format!(".got/objects/{}", &printedhash[..2]))?;
            fs::write(&hashpath, &hash).unwrap();
            println!(
                "Hashed object at: .got/objects/{}/{}",
                &printedhash[..2],
                &printedhash[2..]
            );
        }
        Command::LsTree { treehash } => {
            let file = fs::File::open(format!(
                ".got/objects/{}/{}",
                &treehash[..2],
                &treehash[2..]
            ))
            .context("read header from .got/objects")?;
            let z = ZlibDecoder::new(file);
            let mut z = BufReader::new(z);
            let mut buf = Vec::new();
            z.read_until(0, &mut buf)
                .context("read header from .got/objects")?;
            let header = CStr::from_bytes_with_nul(&buf)
                .expect("know there is exactly one nul, and it's at the end");
            let header = header
                .to_str()
                .context(".got/objects file header isn't valid UTF-8")?;
            let Some((kind, size)) = header.split_once(' ') else {
                anyhow::bail!(
                    ".got/objects file header did not start with a known type: '{header}'"
                );
            };
            let kind = match kind {
                "tree" => Kind::Tree,
                _ => anyhow::bail!("we do not yet know how to print a '{kind}'"),
            };
            let size = size
                .parse::<usize>()
                .context(".got/objects file header has invalid size: {size}")?;
            buf.clear();
            buf.resize(size, 0);
            z.read_exact(&mut buf[..])
                .context("read true contents of .got/objects file")?;
            let n = z
                .read(&mut [0])
                .context("validate EOF in .got/object file")?;
            anyhow::ensure!(n == 0, ".got/object file had {n} trailing bytes");
            let mut start = 0;
            let mut end = 0;
            while (end < size) {
                let item = CStr::from_bytes_until_nul(&buf[start..])
                    .expect("know there is exactly one nul, and it's at the end");
                let item = item
                    .to_str()
                    .context(".got/objects file header isn't valid UTF-8")?;
                let Some((mode, name)) = item.split_once(' ') else {
                    anyhow::bail!(
                        ".got/objects file header did not start with a known type: '{item}'"
                    );
                };
                start = item.as_bytes().to_vec().len() + end + 1;
                end = start + 20;
                let shabuf = &buf[start..end];
                let mut sha = String::new();
                for byte in shabuf {
                    sha.push_str(format!("{:02x}", byte).as_str());
                }
                start = end;

                println!("{mode} tree {sha}\t{name}");
            }
        }
    }
    Ok(())
}

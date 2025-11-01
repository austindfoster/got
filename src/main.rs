use anyhow::{Context, Ok};
use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand};
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use is_executable::IsExecutable;
use std::collections::{HashMap, HashSet};
use std::ffi::CStr;
use std::{fmt, fs};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::str::FromStr;
use sha1::{Sha1,Digest};

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
    WriteTree,
    CommitTree {
        #[clap(short = 'p')]
        has_parent: bool,
        #[clap(short = 'm')]
        inline_message: bool,
        tree_hash: String,
        parent: Option<String>,
        message: Option<String>,
    },
    Add {

    },
    Commit {

    },
    Status {

    },
    Diff {

    },
    Restore {

    },
    Branch {

    },
    Checkout {

    },
    Log {

    },
    Stash {

    },
    Fetch {

    },
    Pull {

    },
    Push {

    },
    Clone {

    }

}

enum Kind {
    Blob,
    Commit,
    Tree,
    Tag,
}

struct Object {
    hash: Vec<u8>,
    kind: Kind,
    size: usize,
    contents: Vec<u8>,
}

struct Commit {
    author: String,
    timestamp: DateTime<Utc>,
    hash: Vec<u8>,
    parent_hash: Option<Vec<u8>>,
    message: String,
}

#[derive(Hash)]
enum State {
    Added,
    Deleted,
    Modified,
    Untracked,
}

impl Object {
    fn read(hash: &String) -> anyhow::Result<Object> {
        let file = fs::File::open(format!(".got/objects/{}/{}", &hash[..2], &hash[2..]))
            .context("read header from .got/objects")?;
        let z = ZlibDecoder::new(file);
        let mut z = BufReader::new(z);
        let mut buf = Vec::new();
        z.read_until(b'\x00', &mut buf)
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

        let mut buf = Vec::new();
        buf.resize(size, 0);
        z.read_exact(&mut buf[..])
            .context("read true contents of .got/objects file")?;
        let n = z.read(&mut [0])
            .context("validate EOF in .got/object file")?;
        anyhow::ensure!(n == 0, ".got/object file had {n} trailing bytes");
        let hash = hex::decode(hash)?;
        Ok(Object {
            hash,
            kind,
            size,
            contents: buf,
        })
    }

    fn write(path: &String, kind: &String, buf: &mut Vec<u8>) -> anyhow::Result<Object> {
        let kind_str = kind.as_str();
        let kind = match kind_str {
            "blob" => Kind::Blob,
            "commit" => Kind::Commit,
            "tree" => Kind::Tree,
            "tag" => Kind::Tag,
            _ => anyhow::bail!("we do not yet know how to print a '{kind}'"),
        };
        match kind {
            Kind::Blob => {
                let mut reader = BufReader::new(fs::File::open(path)?);
                let mut vec: Vec<u8> = vec![];
                let content_length = reader.read_until(b'\x00', &mut vec)?;
                let header = format!("{} {}\0", &kind_str, content_length);
                buf.extend(header.as_bytes());
                buf.extend(vec);
            },
            _ => println!("Who knows what I'll put here")
        }
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        let mut hasher = Sha1::new();
        let size = encoder.write(&buf)?;
        hasher.update(&&buf[..size]);
    
        let compressed = encoder.finish()?;
        let hash: Vec<u8> = hasher.finalize().to_vec();

        let hash_str = hex::encode(&hash);
        let hash_path = format!(".got/objects/{}/{}", &hash_str[..2], &hash_str[2..]);
        fs::create_dir_all(format!(".got/objects/{}", &hash_str[..2]))?;
        fs::write(&hash_path, &compressed).unwrap();
        Ok(Object {
            hash,
            kind,
            size,
            contents: compressed,
        })
    }
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            State::Added => write!(f, "new file"),
            State::Deleted => write!(f, "deleted"),
            State::Modified => write!(f, "modified"),
            State::Untracked => write!(f, ""),
        }
    }
}

fn init() {
    fs::create_dir(".got").unwrap();
    fs::create_dir(".got/objects").unwrap();
    fs::create_dir(".got/refs").unwrap();
    fs::write(".got/HEAD", "ref: refs/heads/main\n").unwrap();
    println!("Initialized got directory");
}

fn cat_file(hash: String) -> anyhow::Result<()> {
    let object = Object::read(&hash)?;
    println!("Contents:\n{}", hex::encode(object.contents));
    Ok(())
}

fn hash_object(path: &String) -> anyhow::Result<Object> {
    let kind = String::from_str("blob")?;
    let mut buf: Vec<u8> = vec![];
    let object = Object::write(&path, &kind, &mut buf)?;
    Ok(object)
}

fn print_tree(buf: Vec<u8>, size: usize) -> anyhow::Result<()> {
    let mut start = 0;
    let mut end = 0;
    while end < size {
        let item = CStr::from_bytes_until_nul(&buf[start..])
            .expect("know there is exactly one nul, and it's at the end");
        let item = item
            .to_str()
            .context(".got/objects file header isn't valid UTF-8")?;
        let Some((mode, name)) = item.split_once(' ') else {
            anyhow::bail!(".got/objects file header did not start with a known type: '{item}'");
        };
        start = item.as_bytes().to_vec().len() + end + 1;
        end = start + 20;
        let shabuf = &buf[start..end];
        let hash = hex::encode(shabuf);
        let object = Object::read(&hash)?;
        let kind = match object.kind {
            Kind::Blob => "blob",
            Kind::Commit => "commit",
            Kind::Tree => "tree",
            Kind::Tag => "tag",
        };
        println!("{mode} {kind} {hash}\t{name}");
        start = end;
    }
    Ok(())
}

fn ls_tree(treehash: String) -> anyhow::Result<()> {
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
        anyhow::bail!(".got/objects file header did not start with a known type: '{header}'");
    };
    match kind {
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
    print_tree(buf, size)?;
    Ok(())
}

fn write_tree(path: &String) -> anyhow::Result<Object> {
    let file = fs::File::open(".gotignore")
    .context("read header from .got/objects")?;
    let reader = BufReader::new(file);
    let mut ignore_list: Vec<String> = vec![];
    for line in reader.lines() {
        ignore_list.push(line?);
    }

    let kind = "tree";
    let mut body_buf = Vec::new();
    for entry in fs::read_dir(path).unwrap() {
        let entry = entry.unwrap();
        let filename = entry.file_name();
        let filename = filename.display().to_string();
        let entry_path = entry.path().display().to_string();
        if ignore_list.contains(&filename) {
            continue;
        }
        let meta = entry.metadata()?;
        let mut mode: &str = "100644";
        let mut hashvec = vec![];
        if meta.is_dir() {
            mode = "040000";
            let tree = write_tree(&entry_path)?;
            hashvec = tree.hash;
        } else if meta.is_symlink() {
            mode = "120000";
            let object = hash_object(&entry_path)?;
            hashvec = object.hash;
        } else if meta.is_file() {
            let file_path = Path::new(&entry_path);
            if file_path.is_executable() {
                mode = "100755";
            }
            let object = hash_object(&entry_path)?;
            hashvec = object.hash;
        };
        let item_string = format!("{mode} {filename}");
        let mut item = item_string.as_bytes().to_vec();
        body_buf.append(&mut item);
        body_buf.push(b'\x00');
        body_buf.append(&mut hashvec);
    }
    let size = body_buf.len();
    let mut header = format!("{} {}", kind, size).as_bytes().to_vec();
    let mut buf = vec![];
    buf.append(&mut header);
    buf.push(b'\x00');
    buf.append(&mut body_buf);
    let kind = String::from_str(kind)?;
    let tree_object = Object::write(path, &kind, &mut buf)?;
    Ok(tree_object)
}

fn add() -> anyhow::Result<()> {
    todo!()
}

fn commit() -> anyhow::Result<()> {
    Ok(())
}

fn commit_tree(has_parent: bool, inline_message: bool, tree_hash: String, parent: Option<String>, message: Option<String>) -> anyhow::Result<Object> {
    let kind = "commit";
    let author: String = String::from_str("afoster")?;
    let timestamp = Utc::now();
    let hash = hex::decode(tree_hash)?;
    let mut parent_hash: Option<Vec<u8>> = None;
    let m: String;
    if inline_message {
        m = message.unwrap();
    } else {
        m = create_message();
    }
    let kind = String::from_str(kind)?;
    let path = String::from_str("")?;
    let mut body: Vec<u8> = vec![];
    body.extend(&hash);
    body.extend("\x00author ".as_bytes());
    body.extend(author.as_bytes());
    body.extend("\x00timestamp ".as_bytes());
    body.extend(timestamp.to_string().as_bytes());
    body.extend("\x00message ".as_bytes());
    body.extend(m.as_bytes());
    if has_parent {
        body.extend("\x00parent ".as_bytes());
        let bytes = hex::decode(parent.unwrap())?;
        parent_hash = Some(bytes);
        let bytes = parent_hash.as_ref().unwrap();
        body.extend(bytes);
    }
    let content_length = body.len();
    let header = format!("{} {}\0", &kind, content_length);
    let commit = Commit {
        author,
        timestamp,
        hash,
        parent_hash,
        message: m,
    };
    let mut buf: Vec<u8> = vec![];
    buf.extend(header.as_bytes());
    buf.extend(body);
    let commit_object = Object::write(&path, &kind, &mut buf)?;
    Ok(commit_object)
}

fn create_message() -> String {
    todo!()
}

fn status() -> anyhow::Result<()> {
    let mut staged: HashSet<String> = HashSet::new();
    let mut file_states: HashMap<String, State> = HashMap::new();
    file_states.insert("testFolder".to_string(), State::Added);
    file_states.insert("deleted.txt".to_string(), State::Deleted);
    file_states.insert("test.txt".to_string(), State::Modified);
    file_states.insert("untracked.txt".to_string(), State::Untracked);
    staged.insert("testFolder".to_string());
    let remote_name = "origin";
    let branch_name = "main";
    println!("On branch {}", branch_name);
    println!("Your branch is up to date with {}/{}", remote_name, branch_name);
    println!("Changes to be commited:");

    println!("\t(use got \"restore --staged <file>...\" to unstage)");
    for filename in staged.iter() {
        println!("\t\t{}:\t{}", file_states[filename], filename);
    }

    println!("Changes not staged for commit:");
    println!("\t(use \"got add/rm <file>...\" to update what will be committed)");
    println!("\t(use \"got restore <file>...\" to discard changes in working directory)");
    
    for (filename, state) in file_states.iter() {
        let tracked = match state {
            State::Untracked => false,
            _ => true
        };
        if !staged.contains(filename) && tracked {
            println!("\t\t{}:\t{}", state, filename);
        }
    }

    println!("Untracked files:");
    println!("\t(use \"got add <file>...\" to include in what will be committed)");
    
    for (filename, state) in file_states.iter() {
        let untracked = match state {
            State::Untracked => true,
            _ => false
        };
        if !staged.contains(filename) && untracked {
            println!("\t\t{}", filename);
        }
    }

    Ok(())
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    match args.command {
        Command::Init => init(),
        Command::CatFile { pretty_print, hash } => {
            anyhow::ensure!(
                pretty_print,
                "mode must be given without -p, and we don't support mode"
            );
            cat_file(hash)?;
        }
        Command::HashObject { write, path } => {
            anyhow::ensure!(write, "Only write to file is supported for now");
            let object = hash_object(&path)?;
            println!(
                "{} with contents:\n{}",
                hex::encode(&object.hash),
                hex::encode(&object.contents)
            );
        }
        Command::LsTree { treehash } => ls_tree(treehash)?,
        Command::WriteTree => {
            let path = String::from_str(".")?;
            let tree = write_tree(&path)?;
            println!("{}", hex::encode(&tree.hash))
        },
        Command::Add {  } => add()?,
        Command::Commit { } => commit()?,
        Command::CommitTree { has_parent, inline_message, tree_hash, parent, message } => {
            let commit = commit_tree(has_parent, inline_message, tree_hash, parent, message)?;
            println!("{}", hex::encode(&commit.hash));
        },
        Command::Status { } => status()?,
        _ => println!("There is no matching command for that input"),
    }
    Ok(())
}

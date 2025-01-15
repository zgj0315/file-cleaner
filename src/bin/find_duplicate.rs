use std::{
    collections::HashSet,
    fs::{self, File},
    io::Read,
};

use clap::{arg, command, Parser};
use sha2::{Digest, Sha256};
use sled::Db;
use tokio::sync;
use walkdir::WalkDir;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// dir of file
    #[arg(short, long)]
    dir: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_line_number(true).init();
    let args = Args::parse();
    let sled = sled::open("./data/file.db")?;
    let (tx, mut rx) = sync::mpsc::channel(100);
    let sled_clone = sled.clone();
    let handle = tokio::spawn(async move {
        while let Some((absolute_path, buf)) = rx.recv().await {
            let sled_clone = sled_clone.clone();
            if let Err(e) = handle_file(absolute_path, buf, sled_clone) {
                log::error!("handle file err: {}", e);
            };
        }
    });
    for entry in WalkDir::new(args.dir) {
        if let Ok(entry) = entry {
            if entry.path().is_file() {
                let absolute_path = fs::canonicalize(entry.path())?;
                if let Some(absolute_path_str) = absolute_path.to_str() {
                    if !absolute_path_str.contains("/.") && !absolute_path_str.contains("/#") {
                        let mut file = File::open(entry.path())?;
                        let mut buf = Vec::new();
                        file.read_to_end(&mut buf)?;
                        // if tx.capacity() > 90 {
                        // log::warn!("tx.capacity(): {}", tx.capacity());
                        // }
                        tx.send((absolute_path_str.to_string(), buf)).await?;
                    }
                }
            }
        }
    }
    handle.await?;
    for kv in sled.iter() {
        let (_k, v) = kv?;
        // let sha256 = String::from_utf8(k.to_vec())?;
        let file_set: HashSet<String> = bincode::deserialize(&v[..])?;
        if file_set.len() > 1 {
            log::info!("same sha256 files: {:?}", file_set);
        }
    }
    Ok(())
}

fn handle_file(absolute_path: String, buf: Vec<u8>, sled: Db) -> anyhow::Result<()> {
    let sha256 = hex::encode_upper(Sha256::digest(buf));
    log::info!("{}-{}", sha256, absolute_path);
    match sled.get(&sha256)? {
        Some(v) => {
            let mut decoded: HashSet<String> = bincode::deserialize(&v[..])?;
            decoded.insert(absolute_path);
            let encoded = bincode::serialize(&decoded)?;
            sled.insert(sha256, encoded)?;
        }
        None => {
            let mut file_set = HashSet::new();
            file_set.insert(absolute_path);
            let encoded = bincode::serialize(&file_set)?;
            sled.insert(sha256, encoded)?;
        }
    };
    Ok(())
}

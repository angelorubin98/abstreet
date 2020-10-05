pub use crate::io::*;
use crate::time::{clear_current_line, prettyprint_time};
use crate::{elapsed_seconds, prettyprint_usize, to_json, Timer, PROGRESS_FREQUENCY_SECONDS};
use instant::Instant;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{stdout, BufReader, BufWriter, Error, ErrorKind, Read, Write};
use std::path::Path;

// TODO Idea: Have a wrapper type DotJSON(...) and DotBin(...) to distinguish raw path strings
fn maybe_write_json<T: Serialize>(path: &str, obj: &T) -> Result<(), Error> {
    if !path.ends_with(".json") {
        panic!("write_json needs {} to end with .json", path);
    }
    std::fs::create_dir_all(std::path::Path::new(path).parent().unwrap())
        .expect("Creating parent dir failed");

    let mut file = File::create(path)?;
    file.write_all(to_json(obj).as_bytes())?;
    Ok(())
}

pub fn write_json<T: Serialize>(path: String, obj: &T) {
    if let Err(err) = maybe_write_json(&path, obj) {
        panic!("Can't write_json({}): {}", path, err);
    }
    println!("Wrote {}", path);
}

pub fn slurp_file(path: &str) -> Result<Vec<u8>, Error> {
    let mut file = File::open(path)?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;
    Ok(buffer)
}

pub fn maybe_read_binary<T: DeserializeOwned>(path: String, timer: &mut Timer) -> Result<T, Error> {
    if !path.ends_with(".bin") {
        panic!("read_binary needs {} to end with .bin", path);
    }

    timer.read_file(&path)?;
    let obj: T =
        bincode::deserialize_from(timer).map_err(|err| Error::new(ErrorKind::Other, err))?;
    Ok(obj)
}

fn maybe_write_binary<T: Serialize>(path: &str, obj: &T) -> Result<(), Error> {
    if !path.ends_with(".bin") {
        panic!("write_binary needs {} to end with .bin", path);
    }

    std::fs::create_dir_all(std::path::Path::new(path).parent().unwrap())
        .expect("Creating parent dir failed");

    let file = BufWriter::new(File::create(path)?);
    bincode::serialize_into(file, obj).map_err(|err| Error::new(ErrorKind::Other, err))
}

pub fn write_binary<T: Serialize>(path: String, obj: &T) {
    if let Err(err) = maybe_write_binary(&path, obj) {
        panic!("Can't write_binary({}): {}", path, err);
    }
    println!("Wrote {}", path);
}

// Just list all things from a directory, return sorted by name, with file extension removed.
pub fn list_all_objects(dir: String) -> Vec<String> {
    let mut results: BTreeSet<String> = BTreeSet::new();
    match std::fs::read_dir(dir) {
        Ok(iter) => {
            for entry in iter {
                let filename = entry.unwrap().file_name();
                let path = Path::new(&filename);
                if path.to_string_lossy().starts_with('.') {
                    continue;
                }
                let name = path
                    .file_stem()
                    .unwrap()
                    .to_os_string()
                    .into_string()
                    .unwrap();
                results.insert(name);
            }
        }
        Err(ref e) if e.kind() == ErrorKind::NotFound => {}
        Err(e) => panic!(e),
    };
    results.into_iter().collect()
}

// Load all serialized things from a directory, return sorted by name, with file extension removed.
// Detects JSON or binary. Filters out broken files.
pub fn load_all_objects<T: DeserializeOwned>(dir: String) -> Vec<(String, T)> {
    let mut timer = Timer::new(format!("load_all_objects from {}", dir));
    let mut tree: BTreeMap<String, T> = BTreeMap::new();
    match std::fs::read_dir(&dir) {
        Ok(iter) => {
            for entry in iter {
                let filename = entry.unwrap().file_name();
                let path = Path::new(&filename);
                let path_str = path.to_string_lossy();
                if path_str.starts_with('.') {
                    continue;
                }
                let full_path = format!("{}/{}", dir, path_str);
                let name = path
                    .file_stem()
                    .unwrap()
                    .to_os_string()
                    .into_string()
                    .unwrap();
                let maybe_load: Result<T, Error> = if path_str.ends_with(".json") {
                    maybe_read_json(full_path.clone(), &mut timer)
                } else if path_str.ends_with(".bin") {
                    maybe_read_binary(full_path.clone(), &mut timer)
                } else {
                    panic!("Don't know what {} is", full_path);
                };
                match maybe_load {
                    Ok(x) => {
                        tree.insert(name, x);
                    }
                    Err(err) => {
                        println!("Couldn't load {}: {}", full_path, err);
                    }
                }
            }
        }
        Err(ref e) if e.kind() == ErrorKind::NotFound => {}
        Err(e) => panic!(e),
    };
    tree.into_iter().collect()
}

// TODO I'd like to get rid of this and just use Timer.read_file, but external libraries consume
// the reader. :\
pub struct FileWithProgress {
    inner: BufReader<File>,

    path: String,
    processed_bytes: usize,
    total_bytes: usize,
    started_at: Instant,
    last_printed_at: Instant,
}

impl FileWithProgress {
    // Also hands back a callback that'll add the final result to the timer. The caller must run
    // it.
    // TODO It's really a FnOnce, but I don't understand the compiler error.
    pub fn new(path: &str) -> Result<(FileWithProgress, Box<dyn Fn(&mut Timer)>), Error> {
        let file = File::open(path)?;
        let path_copy = path.to_string();
        let total_bytes = file.metadata()?.len() as usize;
        let start = Instant::now();
        Ok((
            FileWithProgress {
                inner: BufReader::new(file),
                path: path.to_string(),
                processed_bytes: 0,
                total_bytes,
                started_at: start,
                last_printed_at: start,
            },
            Box::new(move |ref mut timer| {
                let elapsed = elapsed_seconds(start);
                timer.add_result(
                    elapsed,
                    format!(
                        "Reading {} ({} MB)... {}",
                        path_copy,
                        prettyprint_usize(total_bytes / 1024 / 1024),
                        prettyprint_time(elapsed)
                    ),
                );
            }),
        ))
    }
}

impl Read for FileWithProgress {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
        let bytes = self.inner.read(buf)?;
        self.processed_bytes += bytes;
        if self.processed_bytes > self.total_bytes {
            panic!(
                "{} is too many bytes read from {}",
                prettyprint_usize(self.processed_bytes),
                self.path
            );
        }

        let done = self.processed_bytes == self.total_bytes && bytes == 0;
        if elapsed_seconds(self.last_printed_at) >= PROGRESS_FREQUENCY_SECONDS || done {
            self.last_printed_at = Instant::now();
            clear_current_line();
            if done {
                // TODO Not seeing this case happen!
                println!(
                    "Read {} ({})... {}",
                    self.path,
                    prettyprint_usize(self.total_bytes / 1024 / 1024),
                    prettyprint_time(elapsed_seconds(self.started_at))
                );
            } else {
                print!(
                    "Reading {}: {}/{} MB... {}",
                    self.path,
                    prettyprint_usize(self.processed_bytes / 1024 / 1024),
                    prettyprint_usize(self.total_bytes / 1024 / 1024),
                    prettyprint_time(elapsed_seconds(self.started_at))
                );
                stdout().flush().unwrap();
            }
        }

        Ok(bytes)
    }
}

pub fn list_dir(dir: &std::path::Path) -> Vec<String> {
    let mut files: Vec<String> = Vec::new();
    match std::fs::read_dir(dir) {
        Ok(iter) => {
            for entry in iter {
                files.push(entry.unwrap().path().to_str().unwrap().to_string());
            }
        }
        Err(ref e) if e.kind() == ErrorKind::NotFound => {}
        Err(e) => panic!("Couldn't read_dir {:?}: {}", dir, e),
    };
    files.sort();
    files
}

pub fn file_exists<I: Into<String>>(path: I) -> bool {
    Path::new(&path.into()).exists()
}

// Idempotent
pub fn delete_file<I: Into<String>>(path: I) {
    let path = path.into();
    if std::fs::remove_file(&path).is_ok() {
        println!("Deleted {}", path);
    } else {
        println!("{} doesn't exist, so not deleting it", path);
    }
}

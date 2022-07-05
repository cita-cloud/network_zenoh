use std::{fs, io::Write, path};

pub fn write_to_file(content: &[u8], path: impl AsRef<path::Path>) {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path.as_ref())
        .unwrap();
    file.write_all(content).unwrap();
    file.flush().unwrap();
}

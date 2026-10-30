use std::{
    io::{self, Write},
    path::Path,
};

// Publish only complete files; an existing destination always wins the race.
pub(crate) fn write_new(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut staged = tempfile::NamedTempFile::new_in(parent)?;
    staged.write_all(bytes)?;
    staged.as_file().sync_all()?;
    staged.persist_noclobber(path).map_err(|e| e.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn concurrent_publish_has_one_complete_winner() {
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("result");
        let gate = std::sync::Barrier::new(8);
        let winners = std::thread::scope(|s| {
            let jobs: Vec<_> = (0..8)
                .map(|n| {
                    let path = &path;
                    let gate = &gate;
                    s.spawn(move || {
                        gate.wait();
                        write_new(path, &vec![n; 65536]).is_ok()
                    })
                })
                .collect();
            jobs.into_iter()
                .map(|j| j.join().unwrap())
                .filter(|won| *won)
                .count()
        });
        assert_eq!(winners, 1);
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(bytes.len(), 65536);
        assert!(bytes.iter().all(|b| *b == bytes[0]));
        assert!(write_new(&path, b"replace").is_err());
        assert_eq!(std::fs::read_dir(d.path()).unwrap().count(), 1);
    }
}

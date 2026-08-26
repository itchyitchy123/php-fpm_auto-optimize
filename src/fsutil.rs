use anyhow::{Context, Result, bail};
use std::{fs, io::Read, path::Path};

pub(crate) fn read_limited(path: &Path, limit: u64, kind: &str) -> Result<Vec<u8>> {
    let file =
        fs::File::open(path).with_context(|| format!("could not read {}", path.display()))?;
    if file.metadata()?.len() > limit {
        bail!("{kind} {} exceeds the {limit} byte limit", path.display());
    }

    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        bail!("{kind} {} exceeds the {limit} byte limit", path.display());
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enforces_the_stream_limit() {
        let temporary = tempfile::NamedTempFile::new().unwrap();
        fs::write(temporary.path(), b"12345").unwrap();
        assert!(read_limited(temporary.path(), 4, "fixture").is_err());
        assert_eq!(
            read_limited(temporary.path(), 5, "fixture").unwrap(),
            b"12345"
        );
    }
}

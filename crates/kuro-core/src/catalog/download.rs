//! Resumable file download with integrity checking.
//!
//! Model weights are multi-gigabyte files, so a transfer interrupted by a quit,
//! a sleep or a dropped connection must continue rather than start over. Bytes
//! go to a `.part` file which is only renamed into place once the whole file is
//! present and, when the source publishes a checksum, verified.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use futures::StreamExt;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::{KuroError, Result};

/// Report progress at most this often, so a large download does not generate a
/// database write per network packet.
const PROGRESS_INTERVAL_BYTES: u64 = 4 * 1024 * 1024;

/// Chunk size used when hashing a completed file.
const HASH_CHUNK_BYTES: usize = 1024 * 1024;

pub struct DownloadOutcome {
    pub bytes: u64,
    pub sha256: String,
    pub resumed: bool,
}

/// Download `url` to `dest`, resuming a previous attempt when possible.
///
/// `on_progress` receives `(downloaded_bytes, total_bytes)`. Setting `cancel`
/// stops the transfer and leaves the `.part` file in place for a later resume.
pub async fn download_to_file(
    client: &reqwest::Client,
    url: &str,
    dest: &Path,
    expected_sha256: Option<&str>,
    cancel: Arc<AtomicBool>,
    on_progress: &mut (dyn FnMut(u64, Option<u64>) + Send),
) -> Result<DownloadOutcome> {
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let part_path = part_path_for(dest);
    let existing_bytes = tokio::fs::metadata(&part_path)
        .await
        .map(|meta| meta.len())
        .unwrap_or(0);

    let mut request = client.get(url);
    if existing_bytes > 0 {
        request = request.header(reqwest::header::RANGE, format!("bytes={existing_bytes}-"));
    }

    let response = request.send().await?;
    let status = response.status();

    if status == reqwest::StatusCode::RANGE_NOT_SATISFIABLE {
        // The part file is already at or past the full length; discard it and
        // let the caller retry from a clean slate rather than guessing.
        let _ = tokio::fs::remove_file(&part_path).await;
        return Err(KuroError::model(
            "the partial download was inconsistent with the server and has been discarded; try again",
        ));
    }
    if !status.is_success() {
        return Err(KuroError::model(format!(
            "download failed with HTTP {status}"
        )));
    }

    // A 206 means the server honoured our range; anything else means we are
    // receiving the whole file again and must not append to the old bytes.
    let resumed = existing_bytes > 0 && status == reqwest::StatusCode::PARTIAL_CONTENT;
    let start_at = if resumed { existing_bytes } else { 0 };

    let total_bytes = response
        .content_length()
        .map(|len| len + start_at)
        .or_else(|| content_range_total(&response));

    let mut file = if resumed {
        tokio::fs::OpenOptions::new()
            .append(true)
            .open(&part_path)
            .await?
    } else {
        tokio::fs::File::create(&part_path).await?
    };

    let mut downloaded = start_at;
    let mut last_reported = start_at;
    on_progress(downloaded, total_bytes);

    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        if cancel.load(Ordering::Relaxed) {
            file.flush().await?;
            return Err(KuroError::model("download cancelled"));
        }

        let chunk = chunk?;
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;

        if downloaded - last_reported >= PROGRESS_INTERVAL_BYTES {
            last_reported = downloaded;
            on_progress(downloaded, total_bytes);
        }
    }

    file.flush().await?;
    drop(file);
    on_progress(downloaded, total_bytes);

    // A truncated transfer that ended without an error would otherwise be
    // renamed into place and fail much later, when the model refuses to load.
    if let Some(expected_total) = total_bytes {
        if downloaded != expected_total {
            return Err(KuroError::model(format!(
                "download ended early: got {downloaded} of {expected_total} bytes"
            )));
        }
    }

    let actual_sha256 = sha256_file(&part_path).await?;
    if let Some(expected) = expected_sha256 {
        if !expected.eq_ignore_ascii_case(&actual_sha256) {
            let _ = tokio::fs::remove_file(&part_path).await;
            return Err(KuroError::model(
                "checksum did not match the value published by the source; the file was discarded",
            ));
        }
    }

    tokio::fs::rename(&part_path, dest).await?;

    Ok(DownloadOutcome {
        bytes: downloaded,
        sha256: actual_sha256,
        resumed,
    })
}

fn part_path_for(dest: &Path) -> PathBuf {
    let mut name = dest.file_name().unwrap_or_default().to_os_string();
    name.push(".part");
    dest.with_file_name(name)
}

/// Total size from a `Content-Range: bytes 100-999/1000` header.
fn content_range_total(response: &reqwest::Response) -> Option<u64> {
    response
        .headers()
        .get(reqwest::header::CONTENT_RANGE)?
        .to_str()
        .ok()?
        .rsplit('/')
        .next()?
        .trim()
        .parse()
        .ok()
}

pub async fn sha256_file(path: &Path) -> Result<String> {
    let mut file = tokio::fs::File::open(path).await?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; HASH_CHUNK_BYTES];

    loop {
        let read = file.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn part_file_sits_next_to_its_destination() {
        let dest = Path::new("/models/qwen/model.gguf");
        assert_eq!(
            part_path_for(dest),
            PathBuf::from("/models/qwen/model.gguf.part")
        );
    }

    #[tokio::test]
    async fn hashes_the_empty_file_to_the_known_sha256() {
        let dir = std::env::temp_dir().join(format!("kuro-hash-{}", uuid::Uuid::new_v4()));
        tokio::fs::create_dir_all(&dir).await.expect("mkdir");
        let path = dir.join("empty.bin");
        tokio::fs::write(&path, b"").await.expect("write");

        // The SHA-256 of zero bytes is a well-known constant, which makes this a
        // real check of the hashing path rather than a self-referential one.
        assert_eq!(
            sha256_file(&path).await.expect("hash"),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );

        tokio::fs::remove_dir_all(&dir).await.ok();
    }

    #[tokio::test]
    async fn hash_is_stable_and_content_sensitive() {
        let dir = std::env::temp_dir().join(format!("kuro-hash2-{}", uuid::Uuid::new_v4()));
        tokio::fs::create_dir_all(&dir).await.expect("mkdir");
        let a = dir.join("a.bin");
        let b = dir.join("b.bin");
        tokio::fs::write(&a, b"same").await.expect("write");
        tokio::fs::write(&b, b"same").await.expect("write");

        assert_eq!(
            sha256_file(&a).await.expect("hash"),
            sha256_file(&b).await.expect("hash")
        );

        tokio::fs::write(&b, b"different").await.expect("write");
        assert_ne!(
            sha256_file(&a).await.expect("hash"),
            sha256_file(&b).await.expect("hash")
        );

        tokio::fs::remove_dir_all(&dir).await.ok();
    }
}

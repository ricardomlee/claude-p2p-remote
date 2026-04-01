//! File system service implementation
//!
//! Provides file browsing, reading, and writing capabilities

use crate::protocol::FileEntry;
use anyhow::{Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use std::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// File system service
#[derive(Clone)]
pub struct FileService {
    /// Root directory for all file operations
    root: Utf8PathBuf,
}

impl FileService {
    /// Create a new file service with the given root directory
    pub fn new(root: Utf8PathBuf) -> Self {
        Self { root }
    }

    /// Create a new file service with current directory as root
    pub fn current_dir() -> Result<Self> {
        let cwd = std::env::current_dir()
            .context("Failed to get current directory")?;
        let root = Utf8PathBuf::from_path_buf(cwd)
            .map_err(|_| anyhow::anyhow!("Current directory is not valid UTF-8"))?;
        Ok(Self { root })
    }

    /// Resolve a path relative to root, preventing directory traversal
    fn resolve_path(&self, path: &str) -> Result<Utf8PathBuf> {
        let path = Utf8Path::new(path);

        // Prevent directory traversal attacks
        if path.components().any(|c| {
            matches!(
                c,
                camino::Component::ParentDir | camino::Component::RootDir
            )
        }) {
            return Err(anyhow::anyhow!("Invalid path: directory traversal not allowed"));
        }

        Ok(self.root.join(path))
    }

    /// List directory contents
    pub async fn list_dir(&self, path: &str) -> Result<Vec<FileEntry>> {
        let full_path = self.resolve_path(path)?;

        let entries = tokio::task::spawn_blocking(move || {
            let mut result = Vec::new();

            for entry in fs::read_dir(&full_path)
                .with_context(|| format!("Failed to read directory: {}", full_path))?
            {
                let entry = entry?;
                let metadata = entry.metadata()?;
                let file_type = if metadata.is_dir() {
                    crate::protocol::FileType::Dir
                } else if metadata.is_symlink() {
                    crate::protocol::FileType::Symlink
                } else {
                    crate::protocol::FileType::File
                };

                result.push(FileEntry {
                    name: entry.file_name().to_string_lossy().to_string(),
                    ty: file_type,
                    size: if metadata.is_file() {
                        Some(metadata.len())
                    } else {
                        None
                    },
                    modified: metadata
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_secs()),
                });
            }

            Ok(result)
        })
        .await??;

        Ok(entries)
    }

    /// Read file content as string
    pub async fn read_file(&self, path: &str) -> Result<String> {
        let full_path = self.resolve_path(path)?;

        let content = tokio::fs::read_to_string(&full_path)
            .await
            .with_context(|| format!("Failed to read file: {}", full_path))?;

        Ok(content)
    }

    /// Read file content as bytes
    pub async fn read_file_bytes(&self, path: &str) -> Result<Vec<u8>> {
        let full_path = self.resolve_path(path)?;

        let content = tokio::fs::read(&full_path)
            .await
            .with_context(|| format!("Failed to read file: {}", full_path))?;

        Ok(content)
    }

    /// Write content to file
    pub async fn write_file(&self, path: &str, content: &str) -> Result<()> {
        let full_path = self.resolve_path(path)?;

        // Create parent directories if they don't exist
        if let Some(parent) = full_path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("Failed to create directory: {}", parent))?;
            }
        }

        tokio::fs::write(&full_path, content)
            .await
            .with_context(|| format!("Failed to write file: {}", full_path))?;

        Ok(())
    }

    /// Write bytes to file
    pub async fn write_file_bytes(&self, path: &str, content: &[u8]) -> Result<()> {
        let full_path = self.resolve_path(path)?;

        // Create parent directories if they don't exist
        if let Some(parent) = full_path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("Failed to create directory: {}", parent))?;
            }
        }

        tokio::fs::write(&full_path, content)
            .await
            .with_context(|| format!("Failed to write file: {}", full_path))?;

        Ok(())
    }

    /// Check if a file exists
    pub fn file_exists(&self, path: &str) -> bool {
        self.resolve_path(path)
            .map(|p| p.exists())
            .unwrap_or(false)
    }

    /// Get file metadata
    pub fn file_metadata(&self, path: &str) -> Result<std::fs::Metadata> {
        let full_path = self.resolve_path(path)?;
        let metadata = fs::metadata(&full_path)
            .with_context(|| format!("Failed to get metadata: {}", full_path))?;
        Ok(metadata)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_list_dir() {
        let temp_dir = TempDir::new().unwrap();
        let service = FileService::new(Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf()).unwrap());

        // Create test files
        fs::create_dir(temp_dir.path().join("subdir")).unwrap();
        fs::write(temp_dir.path().join("test.txt"), "hello").unwrap();

        let entries = service.list_dir("").await.unwrap();
        assert_eq!(entries.len(), 2);
    }

    #[tokio::test]
    async fn test_read_write_file() {
        let temp_dir = TempDir::new().unwrap();
        let service = FileService::new(Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf()).unwrap());

        service.write_file("test.txt", "hello world").await.unwrap();
        let content = service.read_file("test.txt").await.unwrap();
        assert_eq!(content, "hello world");
    }

    #[tokio::test]
    async fn test_path_traversal_blocked() {
        let temp_dir = TempDir::new().unwrap();
        let service = FileService::new(Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf()).unwrap());

        // Try to traverse outside root
        let result = service.resolve_path("../etc/passwd");
        assert!(result.is_err());
    }
}

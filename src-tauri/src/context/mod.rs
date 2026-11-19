pub mod file_loader;
pub mod pdf_extractor;
pub mod resource_cache;
pub mod token_counter;

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use uuid::Uuid;

use resource_cache::{CachedResource, ResourceCache};
use token_counter::{count_tokens, BudgetResource, TokenBudget};

/// A loaded context resource (mirrors the TS ContextResource type).
#[derive(Debug, Clone, Serialize)]
pub struct ContextResource {
    pub id: String,
    pub name: String,
    pub file_type: String,
    pub file_path: String,
    pub size_bytes: u64,
    pub token_count: usize,
    pub preview: String,
    pub loaded_at: String,
}

/// Loads, caches, and serves context text from files.
/// Files stored in %APPDATA%/com.nexq.app/context/
pub struct ContextManager {
    context_dir: PathBuf,
    resources: Vec<ContextResource>,
    cache: ResourceCache,
    custom_instructions: String,
}

impl ContextManager {
    pub fn new() -> Self {
        // Determine the context directory: %APPDATA%/com.nexq.app/context/
        let context_dir = Self::get_context_dir();

        // Ensure the directory exists
        if let Err(e) = fs::create_dir_all(&context_dir) {
            log::warn!("Failed to create context directory: {}", e);
        }

        Self {
            context_dir,
            resources: Vec::new(),
            cache: ResourceCache::new(),
            custom_instructions: String::new(),
        }
    }

    /// Get the context storage directory path.
    fn get_context_dir() -> PathBuf {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            PathBuf::from(appdata)
                .join(crate::profile_paths::APP_ID)
                .join("context")
        } else {
            // Fallback for non-Windows or if APPDATA is not set
            let home = std::env::var("HOME")
                .or_else(|_| std::env::var("USERPROFILE"))
                .unwrap_or_else(|_| ".".to_string());
            PathBuf::from(home)
                .join(".nexq")
                .join("context")
        }
    }

    /// Load a file into the context manager.
    /// Copies the file to the context directory, extracts text, and caches it.
    pub fn load_file(&mut self, file_path: &str) -> Result<ContextResource, String> {
        let source_path = Path::new(file_path);

        if !source_path.exists() {
            return Err(format!("File not found: {}", file_path));
        }

        // Determine file type from extension
        let extension = source_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        if !file_loader::CONTEXT_EXTENSIONS.contains(&extension.as_str()) {
            return Err(format!(
                "Unsupported file type: .{}. Supported: .pdf, .txt, .md, .docx",
                extension
            ));
        }
        let file_type = extension.as_str();

        // Generate a unique resource ID
        let resource_id = Uuid::new_v4().to_string();

        // Get original file name
        let file_name = source_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        // Copy file to context directory with unique name to avoid collisions
        let dest_filename = format!("{}_{}", resource_id, file_name);
        let dest_path = self.context_dir.join(&dest_filename);

        fs::copy(source_path, &dest_path)
            .map_err(|e| format!("Failed to copy file to context directory: {}", e))?;

        // Get file size
        let size_bytes = fs::metadata(&dest_path)
            .map(|m| m.len())
            .unwrap_or(0);

        // Extract text based on file type — clean up copied file on failure
        let text = match file_type {
            "pdf" => pdf_extractor::extract_text_from_pdf(
                dest_path.to_str().unwrap_or(file_path),
            ),
            "txt" | "md" => file_loader::load_text_file(
                dest_path.to_str().unwrap_or(file_path),
            ),
            "docx" => crate::rag::file_processor::extract_docx_text(
                dest_path.to_str().unwrap_or(file_path),
            ),
            _ => Ok(String::new()),
        }
        .map_err(|e| {
            let _ = fs::remove_file(&dest_path);
            e
        })?;

        // Count tokens
        let token_count = count_tokens(&text);

        // Create preview (first 100 chars)
        let preview = if text.len() > 100 {
            let boundary = text
                .char_indices()
                .nth(100)
                .map(|(i, _)| i)
                .unwrap_or(text.len());
            format!("{}...", &text[..boundary])
        } else {
            text.clone()
        };

        // Timestamp
        let loaded_at = chrono::Utc::now().to_rfc3339();

        // Create resource
        let resource = ContextResource {
            id: resource_id.clone(),
            name: file_name,
            file_type: file_type.to_string(),
            file_path: dest_path
                .to_str()
                .unwrap_or("")
                .to_string(),
            size_bytes,
            token_count,
            preview,
            loaded_at: loaded_at.clone(),
        };

        // Cache the extracted text
        self.cache.insert(
            resource_id,
            CachedResource {
                text,
                token_count,
                loaded_at,
            },
        );

        // Store resource metadata
        self.resources.push(resource.clone());

        log::info!(
            "Loaded context file: {} ({} tokens)",
            resource.name,
            resource.token_count
        );

        Ok(resource)
    }

    /// Restore a previously-persisted resource from stored metadata.
    ///
    /// Re-extracts the text from the copied file in the context directory so
    /// the in-memory cache is rebuilt after a restart. Skips silently if the
    /// file is no longer on disk (returns Err so the caller can clean up DB).
    pub fn restore_resource(&mut self, resource: ContextResource) -> Result<(), String> {
        let path = Path::new(&resource.file_path);
        if !path.exists() {
            return Err(format!("Context file missing from disk: {}", resource.file_path));
        }

        // Re-extract text (needed for context-stuffing mode and token budget)
        let text = match resource.file_type.as_str() {
            "pdf" => pdf_extractor::extract_text_from_pdf(&resource.file_path)?,
            "txt" | "md" => file_loader::load_text_file(&resource.file_path)?,
            "docx" => crate::rag::file_processor::extract_docx_text(&resource.file_path)?,
            _ => String::new(),
        };

        let token_count = count_tokens(&text);

        self.cache.insert(
            resource.id.clone(),
            CachedResource {
                text,
                token_count,
                loaded_at: resource.loaded_at.clone(),
            },
        );

        // Guard against duplicate restore (idempotent)
        if !self.resources.iter().any(|r| r.id == resource.id) {
            self.resources.push(resource);
        }

        Ok(())
    }

    /// Remove only the managed copy and index for a confirmed resource.
    pub fn remove_file(&mut self, id: &str, conn: Option<&rusqlite::Connection>) -> Result<bool, String> {
        let Some(index) = self.resources.iter().position(|r| r.id == id) else {
            return Ok(false);
        };
        let resource = &self.resources[index];
        let path = Path::new(&resource.file_path);
        if path.parent() != Some(self.context_dir.as_path()) {
            return Err(format!("{} is outside the context directory", path.display()));
        }

        let tx = conn.map(|db| db.unchecked_transaction()).transpose().map_err(|e| e.to_string())?;
        if let Some(tx) = &tx {
            crate::db::rag::delete_chunks_by_file(tx, id)?;
            tx.execute("DELETE FROM context_resources WHERE id = ?1", [id]).map_err(|e| e.to_string())?;
        }
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("Could not remove {}: {}", resource.name, e)),
        }
        if let Some(tx) = tx {
            tx.commit().map_err(|e| e.to_string())?;
        }
        self.resources.remove(index);
        self.cache.remove(id);
        Ok(true)
    }

    /// List all loaded context resources.
    pub fn list_resources(&self) -> Vec<ContextResource> {
        self.resources.clone()
    }

    /// Get the assembled context text (concatenation of all resources' text).
    pub fn get_assembled_context(&self) -> String {
        let mut parts: Vec<String> = Vec::new();

        // Add custom instructions first if present
        if !self.custom_instructions.is_empty() {
            parts.push(format!(
                "## Custom Instructions\n{}\n",
                self.custom_instructions
            ));
        }

        // Add each resource's text
        for resource in &self.resources {
            if let Some(cached) = self.cache.get(&resource.id) {
                if !cached.text.is_empty() {
                    parts.push(format!(
                        "## {} ({})\n{}\n",
                        resource.name, resource.file_type, cached.text
                    ));
                }
            }
        }

        parts.join("\n")
    }

    /// Get only the attached resources' text (without custom instructions).
    /// Used for the context-stuffing fallback so attached documents reach the
    /// LLM even when the RAG index is empty or the embedding server is down.
    pub fn get_resources_context(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        for resource in &self.resources {
            if let Some(cached) = self.cache.get(&resource.id) {
                if !cached.text.is_empty() {
                    parts.push(format!(
                        "## {} ({})\n{}\n",
                        resource.name, resource.file_type, cached.text
                    ));
                }
            }
        }
        parts.join("\n")
    }

    /// Set custom instructions text.
    pub fn set_custom_instructions(&mut self, text: &str) {
        self.custom_instructions = text.to_string();
    }

    /// Get the custom instructions text.
    pub fn get_custom_instructions(&self) -> &str {
        &self.custom_instructions
    }

    /// Compute the token budget breakdown.
    pub fn get_token_budget(
        &self,
        model_context_window: u64,
        transcript_tokens: usize,
    ) -> TokenBudget {
        let budget_resources: Vec<BudgetResource> = self
            .resources
            .iter()
            .map(|r| BudgetResource {
                name: r.name.clone(),
                file_type: r.file_type.clone(),
                token_count: r.token_count,
            })
            .collect();

        token_counter::compute_budget(
            &budget_resources,
            &self.custom_instructions,
            transcript_tokens,
            model_context_window,
        )
    }
}

#[cfg(test)]
mod removal_tests {
    use super::*;
    use crate::db::{context, migrations, rag};

    #[test]
    fn clearing_uploaded_sources_preserves_originals_and_transcript_index() {
        let root = std::env::temp_dir().join(Uuid::new_v4().to_string());
        let context_dir = root.join("context");
        fs::create_dir_all(&context_dir).unwrap();
        let original = root.join("wiki.md");
        fs::write(&original, "wiki content").unwrap();
        let mut manager = ContextManager {
            context_dir, resources: vec![], cache: ResourceCache::new(), custom_instructions: "Speak Chinese".into(),
        };
        let file = manager.load_file(original.to_str().unwrap()).unwrap();
        let db = rusqlite::Connection::open_in_memory().unwrap();
        migrations::run(&db).unwrap();
        db.execute("INSERT INTO context_resources (id,name,file_type,file_path,size_bytes,token_count,preview,loaded_at)
                    VALUES (?1,'wiki.md','md',?2,11,3,'wiki','now')", [&file.id, &file.file_path]).unwrap();
        for (chunk, owner, kind) in [("wiki", file.id.as_str(), "file"), ("speech", "meeting", "transcript")] {
            rag::insert_chunks_batch(&db, &[rag::ChunkRecord {
                chunk_id: chunk.into(), file_id: owner.into(), chunk_index: 0, text: chunk.into(),
                token_count: 1, source_type: kind.into(),
            }]).unwrap();
            rag::store_embedding(&db, chunk, &[1]).unwrap();
        }

        // A failed disk deletion rolls back the index transaction and remains retryable.
        fs::remove_file(&file.file_path).unwrap();
        fs::create_dir(&file.file_path).unwrap();
        assert!(manager.remove_file(&file.id, Some(&db)).is_err());
        assert!(context::has_resource(&db, &file.id).unwrap());
        assert!(manager.get_resources_context().contains("wiki content"));
        fs::remove_dir(&file.file_path).unwrap();
        fs::copy(&original, &file.file_path).unwrap();

        assert!(manager.remove_file(&file.id, Some(&db)).unwrap());
        assert!(!manager.remove_file(&file.id, Some(&db)).unwrap());
        assert!(!context::has_resource(&db, &file.id).unwrap());
        for table in ["rag_chunks", "rag_embeddings"] {
            let remaining: String = db.query_row(&format!("SELECT chunk_id FROM {table}"), [], |row| row.get(0)).unwrap();
            assert_eq!(remaining, "speech");
            let count: i64 = db.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| row.get(0)).unwrap();
            assert_eq!(count, 1);
        }
        assert!(manager.get_resources_context().is_empty());
        assert_eq!(manager.get_custom_instructions(), "Speak Chinese");
        assert!(!Path::new(&file.file_path).exists());
        assert_eq!(fs::read_to_string(original).unwrap(), "wiki content");
        fs::remove_dir_all(root).unwrap();
    }
}

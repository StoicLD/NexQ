use std::fs;
use std::path::Path;

pub const CONTEXT_EXTENSIONS: &[&str] = &["pdf", "txt", "md", "docx"];

#[derive(serde::Serialize, Default)]
pub struct ContextFolder {
    pub files: Vec<std::path::PathBuf>,
    pub errors: Vec<String>,
}

pub fn list_folder(path: &Path) -> Result<ContextFolder, String> {
    if !path.is_dir() {
        return Err(format!("Cannot open folder: {}", path.display()));
    }

    let mut folder = ContextFolder::default();
    for entry in walkdir::WalkDir::new(path).min_depth(1).sort_by_file_name() {
        match entry {
            Ok(file) if file.file_type().is_file() => {
                let extension = file
                    .path()
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .unwrap_or("");
                if CONTEXT_EXTENSIONS
                    .iter()
                    .any(|ext| ext.eq_ignore_ascii_case(extension))
                {
                    folder.files.push(file.into_path());
                }
            }
            Ok(_) => {}
            Err(error) => folder.errors.push(error.to_string()),
        }
    }
    Ok(folder)
}

/// Load a text file (.txt or .md) and return its contents as a String.
/// Tries UTF-8 first (stripping BOM if present), then falls back to Windows-1252.
pub fn load_text_file(file_path: &str) -> Result<String, String> {
    let path = Path::new(file_path);

    if !path.exists() {
        return Err(format!("File not found: {}", file_path));
    }

    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    if extension != "txt" && extension != "md" {
        return Err(format!(
            "Unsupported text file type: .{}. Expected .txt or .md",
            extension
        ));
    }

    let bytes = fs::read(path).map_err(|e| format!("Failed to read file: {}", e))?;

    // Strip UTF-8 BOM if present (EF BB BF)
    let payload = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        &bytes[3..]
    } else {
        &bytes
    };

    // Try UTF-8 first
    if let Ok(text) = std::str::from_utf8(payload) {
        return Ok(text.to_string());
    }

    // Fallback: Windows-1252 (covers Latin-1 and most Western European encodings)
    log::warn!(
        "File '{}' is not valid UTF-8 — decoding as Windows-1252",
        file_path
    );
    let (text, _, had_errors) = encoding_rs::WINDOWS_1252.decode(payload);
    if had_errors {
        log::warn!(
            "Some characters in '{}' could not be decoded and were replaced",
            file_path
        );
    }
    Ok(text.into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_includes_supported_files_from_nested_directories() {
        let root = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
        let nested = root.join("子目录");
        fs::create_dir_all(&nested).unwrap();
        for path in [
            root.join("intro.MD"),
            nested.join("intro.MD"),
            nested.join("notes.Pdf"),
            nested.join("talk.txt"),
            nested.join("spec.docx"),
            nested.join("source.rs"),
        ] {
            fs::write(path, "").unwrap();
        }
        fs::create_dir(root.join("empty")).unwrap();

        let result = list_folder(&root).unwrap();
        assert!(result.errors.is_empty());
        assert_eq!(
            result.files,
            vec![
                root.join("intro.MD"),
                nested.join("intro.MD"),
                nested.join("notes.Pdf"),
                nested.join("spec.docx"),
                nested.join("talk.txt"),
            ]
        );
        assert!(list_folder(&root.join("empty")).unwrap().files.is_empty());
        assert!(list_folder(&root.join("missing")).is_err());
        fs::remove_dir_all(&root).unwrap();
    }
}

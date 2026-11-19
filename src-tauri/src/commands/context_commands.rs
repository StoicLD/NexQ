use tauri::{command, State};

use crate::state::AppState;

#[command]
pub async fn list_context_folder(
    folder_path: std::path::PathBuf,
) -> Result<crate::context::file_loader::ContextFolder, String> {
    tauri::async_runtime::spawn_blocking(move || crate::context::file_loader::list_folder(&folder_path))
        .await
        .map_err(|e| format!("Failed to scan context folder: {}", e))?
}

#[command]
pub async fn load_context_file(
    file_path: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let ctx_mgr = state
        .context
        .as_ref()
        .ok_or_else(|| "Context manager not initialized".to_string())?;

    let mut ctx = ctx_mgr.lock().map_err(|e| e.to_string())?;
    let resource = ctx.load_file(&file_path)?;

    if let Some(db) = &state.database {
        let record = crate::db::context::ContextResource {
            id: resource.id.clone(),
            name: resource.name.clone(),
            file_type: resource.file_type.clone(),
            file_path: resource.file_path.clone(),
            size_bytes: resource.size_bytes as i64,
            token_count: resource.token_count as i64,
            preview: resource.preview.clone(),
            loaded_at: resource.loaded_at.clone(),
        };
        let saved = db.lock().map_err(|e| e.to_string()).and_then(|db| {
            crate::db::context::add_context_resource(db.connection(), &record).map_err(|e| e.to_string())
        });
        if let Err(error) = saved {
            ctx.remove_file(&resource.id, None)?;
            return Err(format!("Could not save {}: {}", resource.name, error));
        }
    }

    serde_json::to_string(&resource)
        .map_err(|e| format!("Failed to serialize resource: {}", e))
}

#[command]
pub async fn remove_context_file(
    resource_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    remove_context_files(vec![resource_id], state).await
}

#[command]
pub async fn remove_context_files(
    resource_ids: Vec<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut ctx = state.context.as_ref().ok_or("Context manager not initialized")?
        .lock().map_err(|e| e.to_string())?;
    let db = state.database.as_ref().map(|db| db.lock()).transpose().map_err(|e| e.to_string())?;
    let conn = db.as_ref().map(|db| db.connection());
    let mut errors = Vec::new();
    let mut removed = false;
    for id in resource_ids {
        match ctx.remove_file(&id, conn) {
            Ok(changed) => removed |= changed,
            Err(error) => errors.push(error),
        }
    }
    if removed {
        state.gemini_cache.lock().map_err(|e| e.to_string())?.take();
    }
    if errors.is_empty() { Ok(()) } else { Err(errors.join("\n")) }
}

#[command]
pub async fn list_context_resources(
    state: State<'_, AppState>,
) -> Result<String, String> {
    let ctx_mgr = state
        .context
        .as_ref()
        .ok_or_else(|| "Context manager not initialized".to_string())?;

    let ctx = ctx_mgr
        .lock()
        .map_err(|e| format!("Failed to lock context manager: {}", e))?;

    let resources = ctx.list_resources();

    serde_json::to_string(&resources)
        .map_err(|e| format!("Failed to serialize resources: {}", e))
}

#[command]
pub async fn set_custom_instructions(
    instructions: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let ctx_mgr = state
        .context
        .as_ref()
        .ok_or_else(|| "Context manager not initialized".to_string())?;

    let mut ctx = ctx_mgr
        .lock()
        .map_err(|e| format!("Failed to lock context manager: {}", e))?;

    ctx.set_custom_instructions(&instructions);

    Ok(())
}

#[command]
pub async fn get_assembled_context(
    state: State<'_, AppState>,
) -> Result<String, String> {
    let ctx_mgr = state
        .context
        .as_ref()
        .ok_or_else(|| "Context manager not initialized".to_string())?;

    let ctx = ctx_mgr
        .lock()
        .map_err(|e| format!("Failed to lock context manager: {}", e))?;

    Ok(ctx.get_assembled_context())
}

#[command]
pub async fn get_token_budget(
    state: State<'_, AppState>,
) -> Result<String, String> {
    let ctx_mgr = state
        .context
        .as_ref()
        .ok_or_else(|| "Context manager not initialized".to_string())?;

    let ctx = ctx_mgr
        .lock()
        .map_err(|e| format!("Failed to lock context manager: {}", e))?;

    // Default model context window: 128k tokens (can be overridden later)
    let model_context_window: u64 = 128_000;
    // Transcript tokens default to 0 when not in a meeting
    let transcript_tokens: usize = 0;

    let budget = ctx.get_token_budget(model_context_window, transcript_tokens);

    serde_json::to_string(&budget)
        .map_err(|e| format!("Failed to serialize token budget: {}", e))
}

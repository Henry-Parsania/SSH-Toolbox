use std::sync::Arc;

use tauri::State;
use toolbox_core::AppError;
use toolbox_core::prompt::{PromptBroker, PromptResponse};

/// The user's answer to a question the backend asked while connecting.
#[tauri::command]
pub async fn prompt_respond(
    broker: State<'_, Arc<PromptBroker>>,
    prompt_id: String,
    response: PromptResponse,
) -> Result<(), AppError> {
    broker.respond(&prompt_id, response)
}

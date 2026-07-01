use crate::function_tool::FunctionCallError;
use crate::tools::context::{boxed_tool_output, ToolInvocation, ToolOutput, ToolPayload};
use crate::tools::handlers::{parse_arguments, resolve_tool_environment};
use crate::tools::registry::CoreToolRuntime;
use crate::tools::registry::ToolExecutor;
use codex_protocol::models::{FunctionCallOutputPayload, ResponseInputItem};
use codex_tools::{JsonSchema, ResponsesApiTool, ToolName, ToolSpec};
use serde::Deserialize;
use serde_json::Value as JsonValue;
use std::collections::BTreeMap;

pub struct FileWriteHandler;

#[derive(Deserialize)]
struct FileWriteArgs {
    path: String,
    content: String,
}

pub struct FileWriteOutput {
    message: String,
}

impl ToolOutput for FileWriteOutput {
    fn log_preview(&self) -> String {
        self.message.clone()
    }

    fn success_for_logging(&self) -> bool {
        true
    }

    fn to_response_item(&self, call_id: &str, _payload: &ToolPayload) -> ResponseInputItem {
        let mut output = FunctionCallOutputPayload::from_text(self.message.clone());
        output.success = Some(true);

        ResponseInputItem::FunctionCallOutput {
            call_id: call_id.to_string(),
            output,
        }
    }

    fn code_mode_result(&self, _payload: &ToolPayload) -> JsonValue {
        JsonValue::Object(serde_json::Map::new())
    }
}

#[async_trait::async_trait]
impl ToolExecutor<ToolInvocation> for FileWriteHandler {
    fn tool_name(&self) -> ToolName {
        ToolName::plain("file_write")
    }

    fn spec(&self) -> ToolSpec {
        let properties = BTreeMap::from([
            (
                "path".to_string(),
                JsonSchema::string(Some("The relative or absolute path of the file to write".to_string())),
            ),
            (
                "content".to_string(),
                JsonSchema::string(Some("The full content to write to the file".to_string())),
            ),
        ]);

        ToolSpec::Function(ResponsesApiTool {
            name: "file_write".to_string(),
            description: "Writes or overwrites the content of a file.".to_string(),
            strict: false,
            defer_loading: None,
            parameters: JsonSchema::object(
                properties,
                Some(vec!["path".to_string(), "content".to_string()]),
                Some(false.into()),
            ),
            output_schema: None,
        })
    }

    async fn handle(
        &self,
        invocation: ToolInvocation,
    ) -> Result<Box<dyn ToolOutput>, FunctionCallError> {
        let ToolInvocation { turn, payload, .. } = invocation;
        let arguments = match payload {
            ToolPayload::Function { arguments } => arguments,
            _ => {
                return Err(FunctionCallError::RespondToModel(
                    "file_write handler received unsupported payload".to_string(),
                ));
            }
        };

        let args: FileWriteArgs = parse_arguments(&arguments)?;
        let Some(turn_environment) = resolve_tool_environment(turn.as_ref(), None)? else {
            return Err(FunctionCallError::RespondToModel(
                "file_write is unavailable in this environment".to_string(),
            ));
        };

        let cwd = turn_environment.cwd.clone();
        let abs_path = cwd.join(&args.path);
        let sandbox = turn.file_system_sandbox_context(None, &cwd);
        let fs = turn_environment.environment.get_filesystem();

        // Ensure parent directories exist
        if let Some(parent) = abs_path.parent() {
            let parent_abs = cwd.join(parent);
            let _ = fs.create_directory(
                &parent_abs,
                crate::FileSystemCreateDirectoryOptions { recursive: true },
                Some(&sandbox),
            ).await;
        }

        fs.write_file(&abs_path, args.content.into_bytes(), Some(&sandbox))
            .await
            .map_err(|e| {
                FunctionCallError::RespondToModel(format!(
                    "Failed to write file `{}`: {e}",
                    args.path
                ))
            })?;

        Ok(boxed_tool_output(FileWriteOutput {
            message: format!("Successfully wrote file `{}`", args.path),
        }))
    }
}

impl CoreToolRuntime for FileWriteHandler {}

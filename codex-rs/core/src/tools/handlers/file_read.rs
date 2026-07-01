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

pub struct FileReadHandler;

#[derive(Deserialize)]
struct FileReadArgs {
    path: String,
    start_line: Option<usize>,
    end_line: Option<usize>,
}

pub struct FileReadOutput {
    content: String,
}

impl ToolOutput for FileReadOutput {
    fn log_preview(&self) -> String {
        self.content.clone()
    }

    fn success_for_logging(&self) -> bool {
        true
    }

    fn to_response_item(&self, call_id: &str, _payload: &ToolPayload) -> ResponseInputItem {
        let mut output = FunctionCallOutputPayload::from_text(self.content.clone());
        output.success = Some(true);

        ResponseInputItem::FunctionCallOutput {
            call_id: call_id.to_string(),
            output,
        }
    }

    fn code_mode_result(&self, _payload: &ToolPayload) -> JsonValue {
        JsonValue::String(self.content.clone())
    }
}

#[async_trait::async_trait]
impl ToolExecutor<ToolInvocation> for FileReadHandler {
    fn tool_name(&self) -> ToolName {
        ToolName::plain("file_read")
    }

    fn spec(&self) -> ToolSpec {
        let properties = BTreeMap::from([
            (
                "path".to_string(),
                JsonSchema::string(Some("The relative or absolute path of the file to read".to_string())),
            ),
            (
                "start_line".to_string(),
                JsonSchema::integer(Some("The 1-indexed starting line number (inclusive) to read".to_string())),
            ),
            (
                "end_line".to_string(),
                JsonSchema::integer(Some("The 1-indexed ending line number (inclusive) to read".to_string())),
            ),
        ]);

        ToolSpec::Function(ResponsesApiTool {
            name: "file_read".to_string(),
            description: "Reads the content of a file, with optional line range pagination.".to_string(),
            strict: false,
            defer_loading: None,
            parameters: JsonSchema::object(
                properties,
                Some(vec!["path".to_string()]),
                Some(false.into()),
            ),
            output_schema: None,
        })
    }

    fn supports_parallel_tool_calls(&self) -> bool {
        true
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
                    "file_read handler received unsupported payload".to_string(),
                ));
            }
        };

        let args: FileReadArgs = parse_arguments(&arguments)?;
        let Some(turn_environment) = resolve_tool_environment(turn.as_ref(), None)? else {
            return Err(FunctionCallError::RespondToModel(
                "file_read is unavailable in this environment".to_string(),
            ));
        };

        let cwd = turn_environment.cwd.clone();
        let abs_path = cwd.join(&args.path);
        let sandbox = turn.file_system_sandbox_context(None, &cwd);
        let fs = turn_environment.environment.get_filesystem();

        let raw_content = fs
            .read_file_text(&abs_path, Some(&sandbox))
            .await
            .map_err(|e| {
                FunctionCallError::RespondToModel(format!(
                    "Failed to read file `{}`: {e}",
                    args.path
                ))
            })?;

        let sliced_content = if args.start_line.is_some() || args.end_line.is_some() {
            let lines: Vec<&str> = raw_content.lines().collect();
            let start = args.start_line.unwrap_or(1).saturating_sub(1);
            let end = args.end_line.unwrap_or(lines.len()).min(lines.len());
            if start > lines.len() || start >= end {
                String::new()
            } else {
                lines[start..end].join("\n")
            }
        } else {
            raw_content
        };

        Ok(boxed_tool_output(FileReadOutput {
            content: sliced_content,
        }))
    }
}

impl CoreToolRuntime for FileReadHandler {}

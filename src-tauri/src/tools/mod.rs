pub mod ask_user_question;
pub mod shell;

use crate::llm::{ToolDefinition, ToolFunction};
use crate::platform;
use serde_json::Value;

/// Names a model may use for the shell tool other than the one we advertise.
/// Both platform spellings are accepted everywhere: models have strong priors
/// about `run_powershell`, and a session recorded on Windows may be resumed on
/// a Mac (or vice versa) with the old name embedded in its history.
pub const SHELL_TOOL_ALIASES: &[&str] = &["run_powershell", "run_shell"];

/// Result from executing a tool.
#[derive(Debug)]
pub struct ToolResult {
    pub output: String,
    pub is_error: bool,
}

/// Trait that all tools must implement.
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters_schema(&self) -> Value;

    /// Whether this tool requires user approval before execution.
    fn requires_approval(&self) -> bool {
        true
    }

    /// Validate tool arguments before execution.
    fn validate_input(&self, arguments: &Value) -> Result<(), String>;

    /// Execute the tool with validated arguments.
    fn execute(&self, arguments: &Value, cwd: Option<&str>) -> ToolResult;
}

/// Registry that holds all available tools and provides lookup/dispatch.
pub struct ToolRegistry {
    tools: Vec<Box<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self { tools: Vec::new() }
    }

    pub fn register(&mut self, tool: Box<dyn Tool>) {
        self.tools.push(tool);
    }

    /// Look up a tool by name.
    pub fn get(&self, name: &str) -> Option<&dyn Tool> {
        self.tools
            .iter()
            .find(|t| t.name() == name)
            .map(|t| t.as_ref())
            .or_else(|| self.get_by_alias(name))
    }

    /// Resolve a cross-platform spelling of the shell tool. Kept separate from
    /// `get` so `definitions()` still advertises exactly one name to the LLM.
    fn get_by_alias(&self, name: &str) -> Option<&dyn Tool> {
        if !SHELL_TOOL_ALIASES.contains(&name) {
            return None;
        }
        self.tools
            .iter()
            .find(|t| t.name() == platform::SHELL.tool_name)
            .map(|t| t.as_ref())
    }

    /// Generate LLM-facing tool definitions for all registered tools.
    pub fn definitions(&self) -> Vec<ToolDefinition> {
        self.tools
            .iter()
            .map(|t| ToolDefinition {
                r#type: "function".to_string(),
                function: ToolFunction {
                    name: t.name().to_string(),
                    description: t.description().to_string(),
                    parameters: t.parameters_schema(),
                },
            })
            .collect()
    }

    /// Validate input then execute a tool by name. Returns error if tool not found or input invalid.
    pub fn validate_and_execute(
        &self,
        name: &str,
        arguments: &Value,
        cwd: Option<&str>,
    ) -> Result<ToolResult, String> {
        let tool = self
            .get(name)
            .ok_or_else(|| format!("Unknown tool: {}", name))?;
        tool.validate_input(arguments)?;
        Ok(tool.execute(arguments, cwd))
    }

}

/// Build the default registry with all built-in tools.
pub fn build_default_registry() -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    registry.register(Box::new(shell::ShellTool));
    registry.register(Box::new(ask_user_question::AskUserQuestionTool));
    registry
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The advertised shell-tool name on the platform running the tests.
    fn shell_tool() -> &'static str {
        platform::SHELL.tool_name
    }

    #[test]
    fn test_registry_lookup() {
        let registry = build_default_registry();
        assert!(registry.get(shell_tool()).is_some());
        assert!(registry.get("nonexistent_tool").is_none());
    }

    #[test]
    fn test_registry_definitions() {
        let registry = build_default_registry();
        let defs = registry.definitions();
        let names: Vec<&str> = defs.iter().map(|d| d.function.name.as_str()).collect();
        assert!(names.contains(&shell_tool()));
        assert!(names.contains(&"ask_user_question"));
        // Exactly one shell tool is advertised, whatever the aliases accept.
        assert_eq!(
            names.iter().filter(|n| SHELL_TOOL_ALIASES.contains(n)).count(),
            1
        );
        let ps = defs
            .iter()
            .find(|d| d.function.name == shell_tool())
            .unwrap();
        assert_eq!(ps.r#type, "function");
        // Schema should have "command" as a required property
        let params = &ps.function.parameters;
        assert_eq!(params["required"][0], "command");
        assert_eq!(params["properties"]["command"]["type"], "string");
    }

    #[test]
    fn test_tool_requires_approval() {
        let registry = build_default_registry();
        let tool = registry.get(shell_tool()).unwrap();
        assert!(tool.requires_approval());
    }

    #[test]
    fn test_validate_and_execute_valid() {
        let registry = build_default_registry();
        // Simple echo command — should succeed
        let args = json!({ "command": "echo hello" });
        let result = registry.validate_and_execute(shell_tool(), &args, None);
        assert!(result.is_ok());
        let r = result.unwrap();
        assert!(r.output.contains("hello"));
        assert!(!r.is_error);
    }

    #[test]
    fn test_validate_and_execute_empty_command() {
        let registry = build_default_registry();
        let args = json!({ "command": "   " });
        let result = registry.validate_and_execute(shell_tool(), &args, None);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("empty"));
    }

    #[test]
    fn test_validate_and_execute_missing_command() {
        let registry = build_default_registry();
        let args = json!({ "wrong_field": "test" });
        let result = registry.validate_and_execute(shell_tool(), &args, None);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing"));
    }

    #[test]
    fn test_validate_and_execute_unknown_tool() {
        let registry = build_default_registry();
        let args = json!({ "command": "echo test" });
        let result = registry.validate_and_execute("fake_tool", &args, None);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown tool"));
    }

    #[test]
    fn test_validate_and_execute_non_string_command() {
        let registry = build_default_registry();
        let args = json!({ "command": 42 });
        let result = registry.validate_and_execute(shell_tool(), &args, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_shell_tool_properties() {
        let tool = shell::ShellTool;
        assert_eq!(tool.name(), platform::SHELL.tool_name);
        assert!(tool.requires_approval());
    }

    #[test]
    fn test_no_tool_definition_names_the_foreign_shell() {
        // Tool names AND descriptions are prompt surface. A macOS build that
        // tells the model "before proposing a PowerShell command" gets
        // PowerShell back — this caught exactly that in ask_user_question.
        let registry = build_default_registry();
        let foreign: &[&str] = if cfg!(target_os = "windows") {
            &["zsh", "POSIX", "run_shell"]
        } else {
            &["PowerShell", "powershell", "run_powershell"]
        };
        for def in registry.definitions() {
            let haystack = format!(
                "{} {} {}",
                def.function.name, def.function.description, def.function.parameters
            );
            for needle in foreign {
                assert!(
                    !haystack.contains(needle),
                    "tool '{}' leaked '{needle}' to the model",
                    def.function.name
                );
            }
        }
    }

    #[test]
    fn test_shell_tool_resolves_cross_platform_aliases() {
        // A history entry written on the other OS must still dispatch.
        let registry = build_default_registry();
        for alias in SHELL_TOOL_ALIASES {
            let tool = registry
                .get(alias)
                .unwrap_or_else(|| panic!("alias {alias} should resolve"));
            assert_eq!(tool.name(), platform::SHELL.tool_name);
        }
    }

    #[test]
    fn test_shell_execute_failing_command() {
        let tool = shell::ShellTool;
        let args = json!({ "command": "exit 1" });
        let result = tool.execute(&args, None);
        assert!(result.is_error);
        assert!(result.output.contains("Failed"));
    }
}

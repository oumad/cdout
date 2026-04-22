pub mod powershell;

use crate::llm::{ToolDefinition, ToolFunction};
use serde_json::Value;

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
    registry.register(Box::new(powershell::PowerShellTool));
    registry
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_registry_lookup() {
        let registry = build_default_registry();
        assert!(registry.get("run_powershell").is_some());
        assert!(registry.get("nonexistent_tool").is_none());
    }

    #[test]
    fn test_registry_definitions() {
        let registry = build_default_registry();
        let defs = registry.definitions();
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].function.name, "run_powershell");
        assert_eq!(defs[0].r#type, "function");
        // Schema should have "command" as a required property
        let params = &defs[0].function.parameters;
        assert_eq!(params["required"][0], "command");
        assert_eq!(params["properties"]["command"]["type"], "string");
    }

    #[test]
    fn test_tool_requires_approval() {
        let registry = build_default_registry();
        let tool = registry.get("run_powershell").unwrap();
        assert!(tool.requires_approval());
    }

    #[test]
    fn test_validate_and_execute_valid() {
        let registry = build_default_registry();
        // Simple echo command — should succeed
        let args = json!({ "command": "echo hello" });
        let result = registry.validate_and_execute("run_powershell", &args, None);
        assert!(result.is_ok());
        let r = result.unwrap();
        assert!(r.output.contains("hello"));
        assert!(!r.is_error);
    }

    #[test]
    fn test_validate_and_execute_empty_command() {
        let registry = build_default_registry();
        let args = json!({ "command": "   " });
        let result = registry.validate_and_execute("run_powershell", &args, None);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("empty"));
    }

    #[test]
    fn test_validate_and_execute_missing_command() {
        let registry = build_default_registry();
        let args = json!({ "wrong_field": "test" });
        let result = registry.validate_and_execute("run_powershell", &args, None);
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
        let result = registry.validate_and_execute("run_powershell", &args, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_powershell_tool_properties() {
        let tool = powershell::PowerShellTool;
        assert_eq!(tool.name(), "run_powershell");
        assert!(tool.requires_approval());
    }

    #[test]
    fn test_powershell_execute_failing_command() {
        let tool = powershell::PowerShellTool;
        let args = json!({ "command": "exit 1" });
        let result = tool.execute(&args, None);
        assert!(result.is_error);
        assert!(result.output.contains("Failed"));
    }
}

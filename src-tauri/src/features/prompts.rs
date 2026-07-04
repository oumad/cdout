//! System-prompt assembly via ordered `PromptSection` builders.
//!
//! Replaces the single `format!` with five interpolations. The previous design
//! had a dual source of truth — the prose said "You have TWO tools: run_powershell
//! and ask_user_question" and the `ToolRegistry::definitions()` JSON schemas
//! shipped to the LLM said the same thing. The new `sends_native_tool_specs`
//! flag lets us skip the prose catalog whenever the client emits native tool
//! schemas (Anthropic/OpenAI/OpenRouter all do).

use crate::constants::MAX_FILES_TO_LIST;
use std::env;
use std::fs::File;
use std::io::Write;

/// Per-turn context every section can read from. Owns no allocation of its
/// own beyond the input data — sections render into a shared String buffer.
pub struct PromptContext<'a> {
    pub working_dir: &'a str,
    pub skills_section: &'a str,
    /// True when the LLM client serializes tools natively (e.g. Anthropic
    /// `tools` array, OpenAI `tools` array). When true, sections may skip the
    /// prose tool catalog and shorter rules.
    pub sends_native_tool_specs: bool,
    /// Pre-computed file-list block. Built once by the builder so each section
    /// can decide whether to embed it.
    files_block: String,
    rules_files_line: String,
    script_template: String,
}

pub trait PromptSection {
    fn render(&self, ctx: &PromptContext) -> Option<String>;
}

// --- Concrete sections ---

struct IdentitySection;
impl PromptSection for IdentitySection {
    fn render(&self, ctx: &PromptContext) -> Option<String> {
        if ctx.sends_native_tool_specs {
            // Client ships tools as JSON schemas; the model can read them
            // directly. Keep the role brief.
            Some(
                "You are a PowerShell automation assistant. Use the run_powershell tool to act, \
                 and use ask_user_question when the request has a meaningful branch."
                    .to_string(),
            )
        } else {
            Some(
                "You are a PowerShell automation assistant. You have TWO tools: \
                 run_powershell and ask_user_question."
                    .to_string(),
            )
        }
    }
}

struct ContextSection;
impl PromptSection for ContextSection {
    fn render(&self, ctx: &PromptContext) -> Option<String> {
        Some(format!(
            "CONTEXT:\n- Working directory: '{}'\n- {}",
            ctx.working_dir, ctx.files_block
        ))
    }
}

struct RulesSection;
impl PromptSection for RulesSection {
    fn render(&self, ctx: &PromptContext) -> Option<String> {
        // Rule 1 is shortened when the client sends native tool specs — the
        // "WRONG: writing a code block" example was only useful for models that
        // didn't see the tool schema directly.
        let rule_1 = if ctx.sends_native_tool_specs {
            "1. YOU MUST CALL A TOOL. Do NOT just write code in your response text."
        } else {
            "1. YOU MUST CALL A TOOL. Do NOT just write code in your response text.\n   \
             - WRONG: Writing a code block in your message\n   \
             - CORRECT: Calling run_powershell with the command, OR ask_user_question if clarification is needed"
        };
        Some(format!(
            "CRITICAL RULES - READ CAREFULLY:\n\n{}\n\n\
            2. Use ONLY PowerShell. Never suggest Python, batch files, or other languages.\n\n\
            3. After briefly stating your plan (1-2 sentences max), IMMEDIATELY call run_powershell.\n\n\
            {}\n\n\
            5. VERIFY EVERY OUTPUT FILE — this is non-negotiable:\n   \
             - After any command that is supposed to CREATE a file, you MUST verify it exists.\n   \
             - The cleanest pattern is to chain the verification into the SAME script:\n     \
                `ffmpeg -i input.mp4 output.mp4; if (-not (Test-Path output.mp4)) {{ throw 'output.mp4 was not created' }}`\n   \
             - PowerShell `[Exit code: 0 — Success]` only means the LAST command in the pipeline returned success. It does NOT mean the file exists. Many tools (ffmpeg, magick, robocopy) print errors to stderr but still exit 0.\n   \
             - If Test-Path returns False, the file DOES NOT EXIST — do not claim it does, do not declare Task Complete. Diagnose with `Get-ChildItem` and retry.\n   \
             - Never fabricate output filenames. If you can't see the file in the tool result or in a directory listing, treat it as missing.\n\n\
            6. WHEN THE ENTIRE TASK IS FINISHED, you MUST say \"Task Complete\". You may only say \"Task Complete\" AFTER you've verified every claimed output file exists (see rule 5).\n\n\
            7. WHEN ERRORS OCCUR - NEVER STOP:\n   \
             - If a command fails, briefly note the issue (1 sentence) then IMMEDIATELY call run_powershell with a FIXED version\n   \
             - Do NOT just explain the problem and stop\n   \
             - Do NOT ask for permission to fix - just fix it\n   \
             - Keep trying until the task succeeds or you've exhausted options\n\n\
            8. SAFETY - DATA PROTECTION:\n   \
             - Do NOT delete original files unless the user EXPLICITLY asks to \"delete\", \"remove\", or \"replace\" them.\n   \
             - Always create NEW output files with a suffix (e.g. \"_processed\", \"_new\").\n   \
             - Never overwrite the input file directly.\n\n\
            9. AMBIGUITY HANDLING — use ask_user_question, not free-form prose:\n   \
             - If the request has a meaningful branch (rename pattern, output format, overwrite vs new file, scope of selection), call ask_user_question with 2-4 concrete options BEFORE proposing PowerShell.\n   \
             - Examples that warrant a question:\n     \
                • \"rename these photos\" → ask: date prefix / sequence number / parent folder name\n     \
                • \"compress these videos\" → ask: format (mp4/mov/webm) and quality (high/medium/low)\n     \
                • \"clean up this folder\" → ask: delete originals / move to subfolder / archive\n   \
             - Do NOT ask for trivial confirmations (\"shall I start?\"). Only ask when the answer changes the command.\n   \
             - Never ask more than ONE question per turn.",
            rule_1, ctx.rules_files_line
        ))
    }
}

struct ExampleSection;
impl PromptSection for ExampleSection {
    fn render(&self, ctx: &PromptContext) -> Option<String> {
        Some(format!(
            "EXAMPLE WORKFLOW:\n\n\
            User: \"reverse these videos\"\n\
            You: \"I'll reverse the video while keeping audio intact using ffmpeg.\"\n\
            [IMMEDIATELY call run_powershell with the script]\n\n\
            SCRIPT TEMPLATE:\n```\n{}\n```\n\n\
            REMEMBER:\n\
            - Call a tool, don't just show code\n\
            - PowerShell only, no Python\n\
            - Ask first when ambiguous, otherwise plan then IMMEDIATE tool call",
            ctx.script_template
        ))
    }
}

struct SkillsSection;
impl PromptSection for SkillsSection {
    fn render(&self, ctx: &PromptContext) -> Option<String> {
        if ctx.skills_section.trim().is_empty() {
            None
        } else {
            Some(ctx.skills_section.trim().to_string())
        }
    }
}

// --- Builder ---

fn default_sections() -> Vec<Box<dyn PromptSection>> {
    vec![
        Box::new(IdentitySection),
        Box::new(ContextSection),
        Box::new(RulesSection),
        Box::new(ExampleSection),
        Box::new(SkillsSection),
    ]
}

fn build_files_block(
    selected_files: &[String],
) -> Result<(String, String, String), String> {
    let file_count = selected_files.len();
    if selected_files.is_empty() {
        return Ok((
            "Selected files: None (operating on directory)".to_string(),
            "4. Process all relevant files in the directory.".to_string(),
            "$files = Get-ChildItem -Path . -Filter *.mp4\nforeach ($file in $files) { ... }"
                .to_string(),
        ));
    }
    if file_count > MAX_FILES_TO_LIST {
        let temp_path = env::temp_dir().join("shuttle_agent_context_files.txt");
        let mut file =
            File::create(&temp_path).map_err(|e| format!("Failed to create temp file: {}", e))?;
        for f in selected_files {
            writeln!(file, "{}", f).map_err(|e| format!("Failed to write to temp file: {}", e))?;
        }
        let temp_path_str = temp_path.to_string_lossy().replace('\\', "/");
        return Ok((
            format!(
                "Selected files ({} total):\n[LIST TRUNCATED]\nThe full list of files has been written to: '{}'",
                file_count, temp_path_str
            ),
            format!(
                "4. READ THE FILE LIST:\n   - The user selected {} files, which is too many to list here.\n   - Access the list using: $files = Get-Content '{}'\n   - Do NOT try to guess the files. READ THE TEXT FILE.",
                file_count, temp_path_str
            ),
            format!(
                "$files = Get-Content '{}'\nforeach ($file in $files) {{ ... }}",
                temp_path_str
            ),
        ));
    }
    let list = selected_files
        .iter()
        .enumerate()
        .map(|(i, f)| format!("{}. {}", i + 1, f))
        .collect::<Vec<_>>()
        .join("\n");
    Ok((
        format!("Selected files ({} total):\n{}", file_count, list),
        format!(
            "4. Process ALL {} files in ONE script using a foreach loop.",
            file_count
        ),
        "$files = @('path1', 'path2')\nforeach ($file in $files) { ... }".to_string(),
    ))
}

/// Build the system prompt. `sends_native_tool_specs` controls verbosity:
/// pass `false` for local models (Ollama) that don't reliably emit native
/// tool calls and benefit from the longer prose tool catalog, `true` for
/// cloud providers that ship native tool schemas alongside the prompt.
pub fn build_system_prompt(
    context_path: &str,
    selected_files: &[String],
    skills_section: &str,
    sends_native_tool_specs: bool,
) -> Result<String, String> {
    let (files_block, rules_files_line, script_template) = build_files_block(selected_files)?;
    let ctx = PromptContext {
        working_dir: context_path,
        skills_section,
        sends_native_tool_specs,
        files_block,
        rules_files_line,
        script_template,
    };
    let mut parts = Vec::new();
    for section in default_sections() {
        if let Some(rendered) = section.render(&ctx) {
            parts.push(rendered);
        }
    }
    Ok(parts.join("\n\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_with_no_files() {
        let s = build_system_prompt("C:/Users/x", &[], "", true).unwrap();
        assert!(s.contains("Working directory: 'C:/Users/x'"));
        assert!(s.contains("None (operating on directory)"));
        assert!(s.contains("Process all relevant files"));
        // No skills section
        assert!(!s.contains("--- AVAILABLE TOOLS & SKILLS ---"));
    }

    #[test]
    fn renders_with_few_files() {
        let files = vec!["a.mp4".to_string(), "b.mp4".to_string()];
        let s = build_system_prompt("C:/x", &files, "", true).unwrap();
        assert!(s.contains("Selected files (2 total)"));
        assert!(s.contains("1. a.mp4"));
        assert!(s.contains("2. b.mp4"));
        assert!(s.contains("Process ALL 2 files"));
    }

    #[test]
    fn renders_with_many_files_via_temp_path() {
        let files: Vec<String> = (0..50).map(|i| format!("file_{}.mp4", i)).collect();
        let s = build_system_prompt("C:/x", &files, "", true).unwrap();
        assert!(s.contains("Selected files (50 total)"));
        assert!(s.contains("LIST TRUNCATED"));
        assert!(s.contains("Get-Content"));
    }

    #[test]
    fn native_tool_specs_shortens_identity_and_rule_1() {
        let with_native = build_system_prompt("C:/x", &[], "", true).unwrap();
        let without_native = build_system_prompt("C:/x", &[], "", false).unwrap();
        // Native version says "Use the run_powershell tool to act" — concise.
        assert!(with_native.contains("Use the run_powershell tool to act"));
        // Non-native version dual-sources tools — "You have TWO tools".
        assert!(without_native.contains("You have TWO tools"));
        // Non-native rule 1 carries the WRONG/CORRECT example, native doesn't.
        assert!(without_native.contains("WRONG: Writing a code block"));
        assert!(!with_native.contains("WRONG: Writing a code block"));
        // Native prompt should be shorter overall.
        assert!(with_native.len() < without_native.len());
    }

    #[test]
    fn skills_section_appended_when_non_empty() {
        let skills = "\n\n--- AVAILABLE TOOLS & SKILLS ---\n## ffmpeg - foo\nbody";
        let s = build_system_prompt("C:/x", &[], skills, true).unwrap();
        assert!(s.contains("--- AVAILABLE TOOLS & SKILLS ---"));
        assert!(s.contains("## ffmpeg - foo"));
    }

    #[test]
    fn safety_rule_present() {
        let s = build_system_prompt("C:/x", &[], "", true).unwrap();
        assert!(s.contains("Do NOT delete original files"));
        assert!(s.contains("\"_processed\""));
    }

    #[test]
    fn output_verification_rule_present() {
        // Reviewer-flagged: Gemma sessions hit "Task Complete" without
        // verifying that output files actually exist. The prompt must
        // demand Test-Path verification and explain that Exit code 0
        // does not imply file existence.
        let s = build_system_prompt("C:/x", &[], "", true).unwrap();
        assert!(s.contains("Test-Path"), "must mandate Test-Path");
        assert!(
            s.contains("Exit code: 0"),
            "must clarify exit code 0 != file exists"
        );
        assert!(
            s.contains("Never fabricate output filenames"),
            "must forbid filename hallucination"
        );
        assert!(
            s.contains("AFTER you've verified"),
            "Task Complete must require prior verification"
        );
    }

    #[test]
    fn ambiguity_rule_present() {
        let s = build_system_prompt("C:/x", &[], "", true).unwrap();
        assert!(s.contains("AMBIGUITY HANDLING"));
        assert!(s.contains("ask_user_question"));
    }

    #[test]
    fn example_workflow_present() {
        let s = build_system_prompt("C:/x", &[], "", true).unwrap();
        assert!(s.contains("EXAMPLE WORKFLOW"));
        assert!(s.contains("SCRIPT TEMPLATE"));
    }
}

use crate::constants::MAX_FILES_TO_LIST;
use std::env;
use std::fs::File;
use std::io::Write;

pub fn get_initial_system_prompt(
    context_path: &str,
    selected_files: &Vec<String>,
    skills_section: &str,
) -> Result<String, String> {
    let file_count = selected_files.len();

    let (files_section, rules_section, script_template) = if selected_files.is_empty() {
        (
            "Selected files: None (operating on directory)".to_string(),
            "4. Process all relevant files in the directory.".to_string(),
            "$files = Get-ChildItem -Path . -Filter *.mp4\nforeach ($file in $files) { ... }"
                .to_string(),
        )
    } else if file_count > MAX_FILES_TO_LIST {
        let temp_path = env::temp_dir().join("shuttle_agent_context_files.txt");
        let mut file =
            File::create(&temp_path).map_err(|e| format!("Failed to create temp file: {}", e))?;

        for f in selected_files {
            writeln!(file, "{}", f).map_err(|e| format!("Failed to write to temp file: {}", e))?;
        }
        let temp_path_str = temp_path.to_string_lossy().replace("\\", "/");

        (
            format!("Selected files ({} total):\n[LIST TRUNCATED]\nThe full list of files has been written to: '{}'", file_count, temp_path_str),
            format!("4. READ THE FILE LIST:\n   - The user selected {} files, which is too many to list here.\n   - Access the list using: $files = Get-Content '{}'\n   - Do NOT try to guess the files. READ THE TEXT FILE.", file_count, temp_path_str),
            format!("$files = Get-Content '{}'\nforeach ($file in $files) {{ ... }}", temp_path_str)
        )
    } else {
        let list = selected_files
            .iter()
            .enumerate()
            .map(|(i, f)| format!("{}. {}", i + 1, f))
            .collect::<Vec<_>>()
            .join("\n");

        (
            format!("Selected files ({} total):\n{}", file_count, list),
            format!(
                "4. Process ALL {} files in ONE script using a foreach loop.",
                file_count
            ),
            "$files = @('path1', 'path2')\nforeach ($file in $files) { ... }".to_string(),
        )
    };

    Ok(format!(
        "You are a PowerShell automation assistant. You have ONE tool: run_powershell.

CONTEXT:
- Working directory: '{}'
- {}

CRITICAL RULES - READ CAREFULLY:

1. YOU MUST CALL THE run_powershell TOOL. Do NOT just write code in your response text.
   - WRONG: Writing a code block in your message
   - CORRECT: Calling the run_powershell function with the command

2. Use ONLY PowerShell. Never suggest Python, batch files, or other languages.

3. After briefly stating your plan (1-2 sentences max), IMMEDIATELY call run_powershell.

{}

5. When the ENTIRE task is finished, you MUST say \"Task Complete\".

6. WHEN ERRORS OCCUR - NEVER STOP:
   - If a command fails, briefly note the issue (1 sentence) then IMMEDIATELY call run_powershell with a FIXED version
   - Do NOT just explain the problem and stop
   - Do NOT ask for permission to fix - just fix it
   - Keep trying until the task succeeds or you've exhausted options

7. SAFETY - DATA PROTECTION:
   - Do NOT delete original files unless the user EXPLICITLY asks to \"delete\", \"remove\", or \"replace\" them.
   - Always create NEW output files with a suffix (e.g. \"_processed\", \"_new\").
   - Never overwrite the input file directly.

EXAMPLE WORKFLOW:

User: \"reverse these videos\"
You: \"I'll reverse the video while keeping audio intact using ffmpeg.\"
[IMMEDIATELY call run_powershell with the script]

SCRIPT TEMPLATE:
```
{}
```

REMEMBER:
- Call the tool, don't just show code
- PowerShell only, no Python
- Brief plan then IMMEDIATE tool call{}",
        context_path, files_section, rules_section, script_template, skills_section
    ))
}

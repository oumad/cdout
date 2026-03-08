use crate::auth::antigravity as antigravity_auth;
use crate::llm::{Message, ToolDefinition};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};

const DEFAULT_PROJECT_ID: &str = "rising-fact-p41fc"; // Fallback

const ANTIGRAVITY_SYSTEM_INSTRUCTION: &str = r#"<identity>
You are Antigravity, a powerful agentic AI coding assistant designed by the Google DeepMind team working on Advanced Agentic Coding.
You are pair programming with a USER to solve their coding task. The task may require creating a new codebase, modifying or debugging an existing codebase, or simply answering a question.
The USER will send you requests, which you must always prioritize addressing. Along with each USER request, we will attach additional metadata about their current state, such as what files they have open and where their cursor is.
This information may or may not be relevant to the coding task, it is up for you to decide.
</identity>

<tool_calling>
Call tools as you normally would. The following list provides additional guidance to help you avoid errors:
  - **Absolute paths only**. When using tools that accept file path arguments, ALWAYS use the absolute file path.
</tool_calling>

<web_application_development>
## Technology Stack
Your web applications should be built using the following technologies:
1. **Core**: Use HTML for structure and JavaScript for logic.
2. **Styling (CSS)**: Use Vanilla CSS for maximum flexibility and control. Avoid using TailwindCSS unless the USER explicitly requests it; in this case, first confirm which TailwindCSS version to use.
3. **Web App**: If the USER specifies that they want a more complex web app, use a framework like Next.js or Vite. Only do this if the USER explicitly requests a web app.
4. **New Project Creation**: If you need to use a framework for a new app, use `npx` with the appropriate script, but there are some rules to follow:
   - Use `npx -y` to automatically install the script and its dependencies
   - You MUST run the command with `--help` flag to see all available options first
   - Initialize the app in the current directory with `./` (example: `npx -y create-vite-app@latest ./`)
   - You should run in non-interactive mode so that the user doesn't need to input anything
5. **Running Locally**: When running locally, use `npm run dev` or equivalent dev server. Only build the production bundle if the USER explicitly requests it or you are validating the code for correctness.

# Design Aesthetics
1. **Use Rich Aesthetics**: The USER should be wowed at first glance by the design. Use best practices in modern web design (e.g. vibrant colors, dark modes, glassmorphism, and dynamic animations) to create a stunning first impression. Failure to do this is UNACCEPTABLE.
2. **Prioritize Visual Excellence**: Implement designs that will WOW the user and feel extremely premium:
   - Avoid generic colors (plain red, blue, green). Use curated, harmonious color palettes (e.g., HSL tailored colors, sleek dark modes).
   - Using modern typography (e.g., from Google Fonts like Inter, Roboto, or Outfit) instead of browser defaults.
   - Use smooth gradients
   - Add subtle micro-animations for enhanced user experience
3. **Use a Dynamic Design**: An interface that feels responsive and alive encourages interaction. Achieve this with hover effects and interactive elements. Micro-animations, in particular, are highly effective for improving user engagement.
4. **Premium Designs**: Make a design that feels premium and state of the art. Avoid creating simple minimum viable products.
5. **Don't use placeholders**: If you need an image, use your generate_image tool to create a working demonstration.

## Implementation Workflow
Follow this systematic approach when building web applications:
1. **Plan and Understand**:
   - Fully understand the user's requirements
   - Draw inspiration from modern, beautiful, and dynamic web designs
   - Outline the features needed for the initial version
2. **Build the Foundation**:
   - Start by creating/modifying `index.css`
   - Implement the core design system with all tokens and utilities
3. **Create Components**:
   - Build necessary components using your design system
   - Ensure all components use predefined styles, not ad-hoc utilities
   - Keep components focused and reusable
4. **Assemble Pages**:
   - Update the main application to incorporate your design and components
   - Ensure proper routing and navigation
   - Implement responsive layouts
5. **Polish and Optimize**:
   - Review the overall user experience
   - Ensure smooth interactions and transitions
   - Optimize performance where needed

## SEO Best Practices
Automatically implement SEO best practices on every page:
- **Title Tags**: Include proper, descriptive title tags for each page
- **Meta Descriptions**: Add compelling meta descriptions that accurately summarize page content
- **Heading Structure**: Use a single `<h1>` per page with proper heading hierarchy
- **Semantic HTML**: Use appropriate HTML5 semantic elements
- **Unique IDs**: Ensure all interactive elements have unique, descriptive IDs for browser testing
- **Performance**: Ensure fast page load times through optimization
CRITICAL REMINDER: AESTHETICS ARE VERY IMPORTANT. If your web app looks simple and basic then you have FAILED!
</web_application_development>
<ephemeral_message>
There will be an <EPHEMERAL_MESSAGE> appearing in the conversation at times. This is not coming from the user, but instead injected by the system as important information to pay attention to. 
Do not respond to nor acknowledge those messages, but do follow them strictly.
</ephemeral_message>

<communication_style>
- **Formatting**. Format your responses in github-style markdown to make your responses easier for the USER to parse. For example, use headers to organize your responses and bolded or italicized text to highlight important keywords. Use backticks to format file, directory, function, and class names. If providing a URL to the user, format this in markdown as well, for example `[label](example.com)`.
- **Proactiveness**. As an agent, you are allowed to be proactive, but only in the course of completing the user's task. For example, if the user asks you to add a new component, you can edit the code, verify build and test statuses, and take any other obvious follow-up actions, such as performing additional research. However, avoid surprising the user. For example, if the user asks HOW to approach something, you should answer their question and instead of jumping into editing a file.
- **Helpfulness**. Respond like a helpful software engineer who is explaining your work to a friendly collaborator on the project. Acknowledge mistakes or any backtracking you do as a result of new information.
- **Ask for clarification**. If you are unsure about the USER's intent, always ask for clarification rather than making assumptions.
</communication_style>"#;

#[derive(Serialize)]
struct AntigravityRequest {
    contents: Vec<Content>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<ToolWrapper>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    generation_config: Option<GenerationConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system_instruction: Option<SystemInstruction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_config: Option<ToolConfig>,
}

#[derive(Serialize)]
struct SystemInstruction {
    role: String,
    parts: Vec<Part>,
}

#[derive(Serialize)]
struct ToolConfig {
    function_calling_config: FunctionCallingConfig,
}

#[derive(Serialize)]
struct FunctionCallingConfig {
    mode: String,
}

#[derive(Serialize)]
struct Content {
    role: String,
    parts: Vec<Part>,
}

#[derive(Serialize)]
struct Part {
    text: String,
}

#[derive(Serialize)]
struct ToolWrapper {
    function_declarations: Vec<GeminiFunctionDeclaration>,
}

#[derive(Serialize)]
struct GeminiFunctionDeclaration {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Serialize)]
struct GenerationConfig {
    max_output_tokens: Option<i32>,
    temperature: Option<f32>,
}

// Response structs (simplified)
#[derive(Deserialize, Debug)]
struct AntigravityResponseChunk {
    response: Option<GenerateContentResponse>,
}

#[derive(Deserialize, Debug)]
struct GenerateContentResponse {
    candidates: Option<Vec<Candidate>>,
}

#[derive(Deserialize, Debug)]
struct Candidate {
    content: Option<CandidateContent>,
}

#[derive(Deserialize, Debug)]
struct CandidateContent {
    parts: Option<Vec<CandidatePart>>,
}

// ... (omitted)

#[derive(Deserialize, Debug)]
struct CandidatePart {
    text: Option<String>,
    #[serde(rename = "functionCall")]
    function_call: Option<GeminiFunctionCall>,
}

#[derive(Deserialize, Debug)]
struct GeminiFunctionCall {
    name: String,
    args: serde_json::Value,
}

pub async fn chat_stream(
    model: &str,
    messages: Vec<Message>,
    tools: Option<Vec<ToolDefinition>>,
    callback: impl Fn(String) + Send + 'static,
) -> Result<Message, String> {
    let access_token = antigravity_auth::get_valid_token().await?;
    let creds = antigravity_auth::load_credentials().ok_or("No credentials")?;
    let project_id = creds.project_id.as_deref().unwrap_or(DEFAULT_PROJECT_ID);

    // Filter out system messages and combine them
    let mut contents = Vec::new();
    let mut system_prompts = Vec::new();

    for msg in messages {
        if msg.role == "system" {
            system_prompts.push(msg.content);
        } else if msg.role == "tool" {
            // Tool results → send as user message with clear labeling
            // so Gemini understands this is the output of a previously executed command
            contents.push(Content {
                role: "user".to_string(),
                parts: vec![Part {
                    text: format!("[Command Output (already executed)]:\n{}", msg.content),
                }],
            });
        } else if msg.role == "assistant" {
            // For assistant messages that had tool_calls, append a description
            // of what was executed so Gemini knows it already ran the command
            let mut text = msg.content.clone();
            if let Some(tool_calls) = &msg.tool_calls {
                for tc in tool_calls {
                    let cmd_summary = if let Some(cmd) = tc
                        .function
                        .arguments
                        .get("command")
                        .and_then(|v| v.as_str())
                    {
                        format!(
                            "\n\n[Already executed {} with command: {}]",
                            tc.function.name, cmd
                        )
                    } else {
                        format!("\n\n[Already executed {}]", tc.function.name)
                    };
                    text.push_str(&cmd_summary);
                }
            }
            contents.push(Content {
                role: "model".to_string(),
                parts: vec![Part { text }],
            });
        } else {
            // User messages
            contents.push(Content {
                role: "user".to_string(),
                parts: vec![Part { text: msg.content }],
            });
        }
    }

    // Construct the combined system instruction
    let combined_system_instruction = format!(
        "{}\n\n{}",
        ANTIGRAVITY_SYSTEM_INSTRUCTION,
        system_prompts.join("\n\n")
    );

    let system_instruction = Some(SystemInstruction {
        role: "user".to_string(),
        parts: vec![Part {
            text: combined_system_instruction,
        }],
    });

    // Add tools wrapper
    let tools_wrapper = tools.map(|t| {
        let declarations = t
            .into_iter()
            .map(|tool| GeminiFunctionDeclaration {
                name: tool.function.name,
                description: tool.function.description,
                parameters: tool.function.parameters,
            })
            .collect();

        vec![ToolWrapper {
            function_declarations: declarations,
        }]
    });

    // Force AUTO mode if tools are present
    let tool_config = if tools_wrapper.is_some() {
        Some(ToolConfig {
            function_calling_config: FunctionCallingConfig {
                mode: "AUTO".to_string(),
            },
        })
    } else {
        None
    };

    let request_body = AntigravityRequest {
        contents,
        tools: tools_wrapper,
        generation_config: Some(GenerationConfig {
            max_output_tokens: Some(8192),
            temperature: Some(0.7),
        }),
        system_instruction,
        tool_config,
    };

    let url = "https://daily-cloudcode-pa.sandbox.googleapis.com/v1internal:streamGenerateContent?alt=sse";

    #[derive(Serialize)]
    struct FullRequest {
        project: String,
        model: String,
        request: AntigravityRequest,
        #[serde(rename = "requestType")]
        request_type: String,
        #[serde(rename = "userAgent")]
        user_agent: String,
        #[serde(rename = "requestId")]
        request_id: String,
    }

    let full_request = FullRequest {
        project: project_id.to_string(),
        model: model.to_string(),
        request: request_body,
        request_type: "agent".to_string(),
        user_agent: "antigravity".to_string(),
        request_id: format!(
            "agent-{}-{}",
            chrono::Utc::now().timestamp_millis(),
            "r4nd0m"
        ),
    };

    let client = reqwest::Client::new();

    // Retry loop for 503 capacity errors
    let max_retries = 3;
    let mut response = None;
    for attempt in 0..=max_retries {
        let res = client.post(url)
            .header("Authorization", format!("Bearer {}", access_token))
            .header("Content-Type", "application/json")
            .header("Accept", "text/event-stream")
            .header("User-Agent", "antigravity/1.15.8 darwin/arm64")
            .header("X-Goog-Api-Client", "google-cloud-sdk vscode_cloudshelleditor/0.1")
            .header("Client-Metadata", r#"{"ideType":"IDE_UNSPECIFIED","platform":"PLATFORM_UNSPECIFIED","pluginType":"GEMINI"}"#)
            .json(&full_request)
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?;

        if res.status().as_u16() == 503 && attempt < max_retries {
            // Capacity exhausted — wait and retry
            let delay_secs = 10u64 * (attempt as u64 + 1);
            callback(format!("\n[Capacity unavailable, retrying in {}s...]\n", delay_secs));
            tokio::time::sleep(std::time::Duration::from_secs(delay_secs)).await;
            continue;
        }

        response = Some(res);
        break;
    }

    let response = response.unwrap();
    let status = response.status();
    if !status.is_success() {
        let text = response.text().await.unwrap_or_default();
        return Err(format!("Antigravity API Error ({}): {}", status, text));
    }

    let mut stream = response.bytes_stream();
    let mut full_content = String::new();
    let mut extracted_tool_calls = Vec::new();

    // Simple SSE parser
    let mut buffer = String::new();
    while let Some(item) = stream.next().await {
        let chunk = item.map_err(|e| format!("Stream error: {}", e))?;
        let s = String::from_utf8_lossy(&chunk);
        buffer.push_str(&s);

        while let Some(pos) = buffer.find('\n') {
            let line = buffer[..pos].trim().to_string();
            buffer = buffer[pos + 1..].to_string();

            if line.starts_with("data: ") {
                let json_str = &line[6..];
                if json_str.trim().is_empty() {
                    continue;
                }

                if let Ok(response_chunk) =
                    serde_json::from_str::<AntigravityResponseChunk>(json_str)
                {
                    if let Some(resp) = response_chunk.response {
                        if let Some(candidates) = resp.candidates {
                            if let Some(candidate) = candidates.first() {
                                if let Some(content) = &candidate.content {
                                    if let Some(parts) = &content.parts {
                                        for part in parts {
                                            if let Some(text) = &part.text {
                                                full_content.push_str(text);
                                                callback(text.clone());
                                            }
                                            if let Some(fc) = &part.function_call {
                                                // Convert GeminiFunctionCall to ToolCall
                                                extracted_tool_calls.push(crate::llm::ToolCall {
                                                    function: crate::llm::FunctionCall {
                                                        name: fc.name.clone(),
                                                        arguments: fc.args.clone(),
                                                    },
                                                });
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(Message {
        role: "assistant".to_string(),
        content: full_content,
        tool_calls: if extracted_tool_calls.is_empty() {
            None
        } else {
            Some(extracted_tool_calls)
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::Message;

    #[tokio::test]
    async fn test_antigravity_connection() {
        let history = vec![Message {
            role: "user".to_string(),
            content: "Hello, what models are available?".to_string(),
            tool_calls: None,
        }];

        // Try Gemini model
        let result = chat_stream(
            "gemini-3-flash",
            history, // Pass history directly, not as a reference
            None,
            |chunk| println!("Chunk: {}", chunk),
        )
        .await;

        match result {
            Ok(msg) => println!("Success! Response: {}", msg.content),
            Err(e) => panic!("Connection failed: {}", e),
        }
    }
}

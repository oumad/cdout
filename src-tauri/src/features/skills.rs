use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SkillRequirements {
    #[serde(default)]
    pub bins: Vec<String>,
    #[serde(default, rename = "anyBins")]
    pub any_bins: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SkillMetadata {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub requires: Option<SkillRequirements>,
}

#[derive(Serialize, Clone, Debug)]
pub struct Skill {
    pub metadata: SkillMetadata,
    pub body: String,
    pub source_path: String,
    pub available: bool,
    pub missing_bins: Vec<String>,
}

fn parse_skill_file(path: &Path) -> Result<(SkillMetadata, String), String> {
    let content =
        std::fs::read_to_string(path).map_err(|e| format!("Failed to read {:?}: {}", path, e))?;

    // Split on YAML frontmatter fences (---)
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return Err(format!("Skill file {:?} missing YAML frontmatter", path));
    }

    let after_first = &trimmed[3..];
    let end = after_first
        .find("\n---")
        .ok_or_else(|| format!("Skill file {:?} missing closing --- fence", path))?;

    let yaml_str = &after_first[..end];
    let body = after_first[end + 4..].trim().to_string();

    let metadata: SkillMetadata =
        serde_yaml::from_str(yaml_str).map_err(|e| format!("YAML parse error in {:?}: {}", path, e))?;

    Ok((metadata, body))
}

fn binary_exists(name: &str) -> bool {
    Command::new("where.exe")
        .arg(name)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn check_requirements(reqs: &SkillRequirements) -> (bool, Vec<String>) {
    let mut missing = Vec::new();

    // ALL bins must exist
    for bin in &reqs.bins {
        if !binary_exists(bin) {
            missing.push(bin.clone());
        }
    }

    // At least ONE of any_bins must exist (if specified)
    if !reqs.any_bins.is_empty() {
        let any_found = reqs.any_bins.iter().any(|b| binary_exists(b));
        if !any_found {
            missing.push(format!(
                "one of [{}]",
                reqs.any_bins.join(", ")
            ));
        }
    }

    let available = missing.is_empty();
    (available, missing)
}

fn load_skills_from_dir(dir: &Path) -> Vec<Skill> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };

    let mut skills = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }

        match parse_skill_file(&path) {
            Ok((metadata, body)) => {
                let (available, missing_bins) = metadata
                    .requires
                    .as_ref()
                    .map(|r| check_requirements(r))
                    .unwrap_or((true, Vec::new()));

                skills.push(Skill {
                    metadata,
                    body,
                    source_path: path.to_string_lossy().to_string(),
                    available,
                    missing_bins,
                });
            }
            Err(e) => {
                eprintln!("Warning: {}", e);
            }
        }
    }

    skills
}

pub fn load_all_skills(resource_dir: Option<PathBuf>) -> Vec<Skill> {
    let mut skills_map = std::collections::HashMap::new();

    // 1. Load bundled skills
    if let Some(res_dir) = resource_dir {
        let bundled_dir = res_dir.join("skills");
        for skill in load_skills_from_dir(&bundled_dir) {
            skills_map.insert(skill.metadata.name.clone(), skill);
        }
    }

    // 2. Load user skills (overrides bundled by name)
    if let Some(config_dir) = dirs::config_dir() {
        let user_dir = config_dir.join("shuttle-io").join("skills");
        for skill in load_skills_from_dir(&user_dir) {
            skills_map.insert(skill.metadata.name.clone(), skill);
        }
    }

    let mut skills: Vec<Skill> = skills_map.into_values().collect();
    skills.sort_by(|a, b| a.metadata.name.cmp(&b.metadata.name));
    skills
}

pub fn get_skills_prompt_section(skills: &[Skill]) -> String {
    let available: Vec<&Skill> = skills.iter().filter(|s| s.available).collect();
    if available.is_empty() {
        return String::new();
    }

    let mut section = String::from("\n\n--- AVAILABLE TOOLS & SKILLS ---\n");
    section.push_str("The following tools are installed on this system. Use them when relevant:\n\n");

    for skill in available {
        section.push_str(&format!("## {} - {}\n", skill.metadata.name, skill.metadata.description));
        section.push_str(&skill.body);
        section.push_str("\n\n");
    }

    section
}

use anyhow::Result;
use std::path::Path;

pub fn run(cwd: &Path, patterns: &[String]) -> Result<()> {
    if patterns.is_empty() {
        anyhow::bail!("specify at least one pattern to untrack");
    }

    let gitattributes_path = cwd.join(".gitattributes");
    if !gitattributes_path.exists() {
        println!("No .gitattributes file found.");
        return Ok(());
    }

    let content = std::fs::read_to_string(&gitattributes_path)?;
    let mut new_lines = Vec::new();
    let mut removed = Vec::new();

    for line in content.lines() {
        let should_remove = patterns
            .iter()
            .any(|pattern| line.starts_with(pattern) && line.contains("filter=hfs"));

        if should_remove {
            let pat = line.split_whitespace().next().unwrap_or("");
            removed.push(pat.to_string());
        } else {
            new_lines.push(line);
        }
    }

    let mut new_content = new_lines.join("\n");
    if !new_content.is_empty() && !new_content.ends_with('\n') {
        new_content.push('\n');
    }
    std::fs::write(&gitattributes_path, new_content)?;

    for pat in &removed {
        println!("Untracking \"{pat}\"");
    }

    if removed.is_empty() {
        println!("No matching patterns found.");
    } else {
        println!("Run `git add .gitattributes` to commit the change.");
    }

    Ok(())
}

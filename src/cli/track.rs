use anyhow::Result;
use std::path::Path;

pub fn run(cwd: &Path, patterns: &[String]) -> Result<()> {
    if patterns.is_empty() {
        print_tracked(cwd)?;
        return Ok(());
    }

    let gitattributes_path = cwd.join(".gitattributes");
    let existing = if gitattributes_path.exists() {
        std::fs::read_to_string(&gitattributes_path)?
    } else {
        String::new()
    };

    let mut content = existing;
    let mut added = Vec::new();

    for pattern in patterns {
        let entry = format!("{pattern} filter=hfs diff=hfs merge=hfs -text");
        if content.lines().any(|line| line.trim() == entry.trim()) {
            println!("Already tracking: {pattern}");
            continue;
        }

        if !content.is_empty() && !content.ends_with('\n') {
            content.push('\n');
        }
        content.push_str(&entry);
        content.push('\n');
        added.push(pattern.clone());
    }

    std::fs::write(&gitattributes_path, content)?;

    for pattern in &added {
        println!("Tracking \"{pattern}\"");
    }

    if !added.is_empty() {
        println!("Run `git add .gitattributes` to commit the tracking configuration.");
    }

    Ok(())
}

fn print_tracked(cwd: &Path) -> Result<()> {
    let gitattributes_path = cwd.join(".gitattributes");
    if !gitattributes_path.exists() {
        println!("No patterns tracked. Run `hfs track \"*.bin\"` to start.");
        return Ok(());
    }

    let content = std::fs::read_to_string(&gitattributes_path)?;
    let mut found = false;

    for line in content.lines() {
        if line.contains("filter=hfs") {
            let pattern = line.split_whitespace().next().unwrap_or("");
            println!("  {pattern}");
            found = true;
        }
    }

    if !found {
        println!("No patterns tracked. Run `hfs track \"*.bin\"` to start.");
    }

    Ok(())
}

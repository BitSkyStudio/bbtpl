use std::{
    collections::{HashMap, HashSet},
    io::{BufRead, Write},
    path::{Path, PathBuf},
    sync::OnceLock,
};

use anyhow::{Result, anyhow};
use regex::Regex;
use walkdir::WalkDir;

fn main() {
    let args: Vec<_> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("expected at least one arguments");
        return;
    }
    if args[1] == "list" {
        let mut templates: Vec<_> = collect_templates().into_keys().collect();
        templates.sort_by_key(|t| t.to_lowercase());
        for template in templates {
            println!("{}", template)
        }
        return;
    }
    let mut overwrite_mode = OverwriteMode::Ask;
    let mut replacements = HashMap::new();
    for argument in args.iter().skip(2) {
        let Some((k, v)) = argument.split_once("=") else {
            eprintln!("expected key value pair, got {}", argument);
            return;
        };
        if k == "overwrite" {
            overwrite_mode = match v {
                "yes" => OverwriteMode::Yes,
                "no" => OverwriteMode::No,
                _ => {
                    eprintln!("overwrite mode should be yes/no");
                    return;
                }
            };
        } else {
            replacements.insert(k.to_string(), v.to_string());
        }
    }
    if let Err(error) = use_template(
        std::env::current_dir().unwrap().as_path(),
        &args[1],
        replacements,
        overwrite_mode,
    ) {
        eprintln!("{:?}", error);
    }
}
fn use_template(
    path: &Path,
    template: &str,
    mut replacements: HashMap<String, String>,
    overwrite_mode: OverwriteMode,
) -> Result<()> {
    let template_path = match collect_templates().remove(template) {
        Some(template) => template,
        _ => return Err(anyhow!("template {template} not found")),
    };
    let mut requested_replacements = HashSet::new();
    extract_replacements(
        &path.as_os_str().to_string_lossy(),
        &mut requested_replacements,
    );
    if !requested_replacements.is_empty() {
        return Err(anyhow!("original path cannot contain replacements"));
    }
    enum TemplateEntry {
        File(String),
        Directory,
    }
    let mut entries = Vec::new();
    for entry in WalkDir::new(&template_path)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        extract_replacements(
            &entry.file_name().to_string_lossy().to_string(),
            &mut requested_replacements,
        );
        let path = entry.path().strip_prefix(&template_path).unwrap();
        let template_entry = if entry.metadata().unwrap().is_file() {
            let content = std::fs::read_to_string(entry.path()).unwrap();
            extract_replacements(&content, &mut requested_replacements);
            TemplateEntry::File(content)
        } else {
            TemplateEntry::Directory
        };
        entries.push((path.to_string_lossy().to_string(), template_entry));
    }
    for requested_replacement in requested_replacements {
        if !replacements.contains_key(&requested_replacement) {
            print!("{requested_replacement}=");
            std::io::stdout().flush().unwrap();
            let value = std::io::stdin().lock().lines().next().unwrap().unwrap();
            replacements.insert(requested_replacement, value);
        }
    }
    for (name, entry) in entries {
        let path = path.join(replace_replacements(name, &replacements));
        match entry {
            TemplateEntry::File(content) => {
                if path.exists() {
                    match overwrite_mode {
                        OverwriteMode::Yes => {}
                        OverwriteMode::No => continue,
                        OverwriteMode::Ask => {
                            let skip;
                            loop {
                                println!("replace file {:?} y/n", path);
                                match std::io::stdin()
                                    .lock()
                                    .lines()
                                    .next()
                                    .unwrap()
                                    .unwrap()
                                    .as_str()
                                {
                                    "y" => {
                                        skip = false;
                                        break;
                                    }
                                    "n" => {
                                        skip = true;
                                        break;
                                    }
                                    _ => {}
                                }
                            }
                            if skip {
                                continue;
                            }
                        }
                    }
                }
                std::fs::write(path, replace_replacements(content, &replacements)).unwrap();
            }
            TemplateEntry::Directory => {
                let _ = std::fs::create_dir(path);
            }
        }
    }
    Ok(())
}
#[derive(Copy, Clone)]
enum OverwriteMode {
    Yes,
    No,
    Ask,
}
fn collect_templates() -> HashMap<String, PathBuf> {
    let mut templates = HashMap::new();
    let mut search_path = std::env::current_dir().unwrap();
    loop {
        if let Ok(found) = std::fs::read_dir(search_path.join(".templates")) {
            for entry in found {
                if let Ok(entry) = entry {
                    if let Ok(metadata) = entry.metadata() {
                        if metadata.is_dir() {
                            templates
                                .entry(entry.file_name().to_string_lossy().to_string())
                                .or_insert(entry.path());
                        }
                    }
                }
            }
        }
        if !search_path.pop() {
            break;
        }
    }
    templates
}
static REPLACEMENT_REGEX: OnceLock<Regex> = OnceLock::new();
fn extract_replacements(input: &str, replacements: &mut HashSet<String>) {
    for replacement in REPLACEMENT_REGEX
        .get_or_init(|| Regex::new(r"§%\{([\w_.]+)\}").unwrap())
        .captures_iter(input)
    {
        replacements.insert(replacement.get(1).unwrap().as_str().to_string());
    }
}
fn replace_replacements(input: String, replacements: &HashMap<String, String>) -> String {
    let mut output = input;
    for replacement in replacements {
        output = output.replace(
            format!("§%{{{}}}", replacement.0).as_str(),
            replacement.1.as_str(),
        );
    }
    output
}

//! A tool to search for Git repositories in a directory and print their remotes.
use std::collections::HashMap;
use std::fs;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use clap::{Parser, ValueEnum};
use serde::Serialize;

/// A directory with a .git/config file and possibly other subdirectories.
#[derive(Clone, Debug, Serialize)]
struct GitDirectory {
    path: PathBuf,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    remotes: HashMap<String, String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    children: Vec<GitDirectory>,
}

/// Print the given Git directory structure in plain text.
/// * `dir` - The directory to print.
/// * `indent` - The number of spaces to indent the output.
fn print_plain(dir: &GitDirectory, indent: usize) {
    println!("{}path: {}", "  ".repeat(indent), dir.path.display());
    if !dir.remotes.is_empty() {
        println!("{}remotes:", "  ".repeat(indent + 1));
        for (name, url) in &dir.remotes {
            println!("{}  {}: {}", "  ".repeat(indent + 1), name, url);
        }
    }
    if !dir.children.is_empty() {
        println!("{}children:", "  ".repeat(indent));
        for child in &dir.children {
            print_plain(child, indent + 1);
        }
    }
}

/// Parse a Git config file.
/// * `config_path` - The path to the Git config file.
fn parse_git_config(config_path: &Path) -> Result<HashMap<String, String>> {
    let file = File::open(config_path)
        .with_context(|| format!("Failed to open Git config file: {:?}", config_path))?;
    let reader = BufReader::new(file);

    let mut remotes = HashMap::new();
    let mut current_remote: Option<String> = None;

    for line in reader.lines() {
        let line = line.context("Failed to read line from Git config")?;
        let line = line.trim();

        if line.starts_with("[remote ") && line.ends_with("]") {
            // strip quotes from remote name
            current_remote = Some(line[8..line.len() - 1].to_string().replace("\"", ""));
        } else if let Some(remote) = line.strip_prefix("url = ") {
            if let Some(name) = &current_remote {
                remotes.insert(name.clone(), remote.to_string());
            }
        }
    }
    Ok(remotes)
}

fn try_get_git_config_remotes(path: &Path) -> Result<Option<HashMap<String, String>>> {
    let git_config = path.join(".git").join("config");
    if git_config.is_file() {
        match parse_git_config(&git_config) {
            Ok(remotes) => Ok(Some(remotes)),
            Err(e) => Err(anyhow!("Error parsing {:?}: {}", git_config, e)),
        }
    } else {
        Ok(None)
    }
}

/// Search for .git/config files in the given directory, optionally recursively.
/// * `dir` - The directory to search in.
/// * `recurse` - Whether to recursively search subdirectories.
fn find_git_configs(dir: &Path, recurse: bool) -> Result<GitDirectory> {
    let mut current_dir = GitDirectory {
        path: dir.to_path_buf(),
        remotes: HashMap::new(),
        children: Vec::new(),
    };
    if let Some(remotes) = try_get_git_config_remotes(dir)? {
        current_dir.remotes = remotes;
    }
    for entry in fs::read_dir(dir).context("Failed to read directory")? {
        let entry = entry.context("Failed to read directory entry")?;
        let path = entry.path();

        if path.is_dir() {
            if recurse {
                let child_dir = find_git_configs(&path, true)?;
                if !child_dir.children.is_empty() || !child_dir.remotes.is_empty() {
                    current_dir.children.push(GitDirectory {
                        path: path.strip_prefix(dir)?.to_path_buf(),
                        remotes: child_dir.remotes,
                        children: child_dir.children,
                    });
                }
            } else if let Some(remotes) = try_get_git_config_remotes(&path)? {
                let child = GitDirectory {
                    path: path.strip_prefix(dir)?.to_path_buf(),
                    remotes,
                    children: Vec::new(),
                };
                current_dir.children.push(child);
            }
        }
    }

    Ok(current_dir)
}

/// The output format to use.
#[derive(Clone, ValueEnum)]
enum OutputFormat {
    Plain,
    Yaml,
    Json,
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Directory to search in (defaults to current directory).
    #[arg(default_value = None)]
    directory: Option<PathBuf>,

    /// Recursively search through subdirectories
    #[arg(short, long)]
    tree: bool,

    /// Output format
    #[arg(short, long, value_enum, default_value = "plain")]
    format: OutputFormat,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let search_dir = match cli.directory {
        Some(dir) => dir,
        None => std::env::current_dir().context("Failed to get current directory")?,
    };

    if !search_dir.is_dir() {
        anyhow::bail!("The specified path is not a directory: {:?}", search_dir);
    }

    let git_structure = find_git_configs(&search_dir, cli.tree)
        .context("Error while searching for .git/config files")?;

    match cli.format {
        OutputFormat::Plain => print_plain(&git_structure, 0),
        OutputFormat::Yaml => {
            let yaml = serde_yaml::to_string(&git_structure)?;
            println!("{}", yaml);
        }
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(&git_structure)?;
            println!("{}", json);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_cmd::Command;
    use predicates::prelude::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::TempDir;

    fn get_binary_name() -> String {
        env!("CARGO_PKG_NAME").to_string()
    }

    fn create_git_config(dir: &Path, content: &str) -> std::io::Result<PathBuf> {
        std::fs::create_dir_all(dir.join(".git"))?;
        let path = dir.join(".git/config");
        let mut file = File::create(path.clone())?;
        file.write_all(content.as_bytes())?;
        Ok(path)
    }

    #[test]
    fn test_parse_git_config_one() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let config_path = create_git_config(
            temp_dir.path(),
            "[remote \"origin\"]\n    url = https://github.com/user/repo.git\n",
        )?;

        let remotes = parse_git_config(&config_path)?;

        assert_eq!(remotes.len(), 1);
        assert_eq!(
            remotes.get("origin"),
            Some(&"https://github.com/user/repo.git".to_string())
        );

        Ok(())
    }

    #[test]
    fn test_parse_git_config() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let config_content = r#"
[remote "origin"]
    url = https://github.com/user/repo.git
[remote "upstream"]
    url = https://github.com/upstream/repo.git
"#;
        create_git_config(temp_dir.path(), config_content)?;

        let config_path = temp_dir.path().join(".git/config");
        // print config path
        println!("{}", config_path.display());
        //print config content
        println!("{}", std::fs::read_to_string(&config_path)?);

        let remotes = parse_git_config(&config_path)?;

        assert_eq!(remotes.len(), 2);
        assert_eq!(
            remotes.get("origin"),
            Some(&"https://github.com/user/repo.git".to_string())
        );
        assert_eq!(
            remotes.get("upstream"),
            Some(&"https://github.com/upstream/repo.git".to_string())
        );

        Ok(())
    }

    #[test]
    fn test_find_git_config_in_subdir() -> Result<()> {
        let temp_dir = TempDir::new()?;
        create_git_config(
            temp_dir.path(),
            "[remote \"origin\"]\n    url = https://github.com/user/repo.git\n",
        )?;

        let sub_dir = temp_dir.path().join("subdir");
        std::fs::create_dir(&sub_dir)?;
        create_git_config(
            &sub_dir,
            "[remote \"origin\"]\n    url = https://github.com/user/subrepo.git\n",
        )?;

        let result = find_git_configs(temp_dir.path(), true)?;
        println!("{:?}", result);
        assert_eq!(result.remotes.len(), 1);
        assert_eq!(
            result.remotes.get("origin"),
            Some(&"https://github.com/user/repo.git".to_string())
        );
        assert_eq!(result.children.len(), 1);

        assert_eq!(result.children[0].remotes.len(), 1);
        assert_eq!(
            result.children[0].remotes.get("origin"),
            Some(&"https://github.com/user/subrepo.git".to_string())
        );
        Ok(())
    }

    #[test]
    fn test_cli_valid_directory() -> Result<()> {
        let temp_dir = TempDir::new()?;
        create_git_config(
            temp_dir.path(),
            "[remote \"origin\"]\n    url = https://github.com/user/repo.git\n",
        )?;

        let mut cmd = Command::cargo_bin(get_binary_name())?;
        cmd.arg(temp_dir.path())
            .assert()
            .success()
            .stdout(predicate::str::contains(
                "origin: https://github.com/user/repo.git",
            ));

        Ok(())
    }

    #[test]
    fn test_cli_invalid_directory() -> Result<()> {
        let mut cmd = Command::cargo_bin(get_binary_name())?;
        cmd.arg("/nonexistent/directory")
            .assert()
            .failure()
            .stderr(predicate::str::contains("not a directory"));

        Ok(())
    }

    #[test]
    fn test_cli_recursive_mode() -> Result<()> {
        let temp_dir = TempDir::new()?;
        create_git_config(
            temp_dir.path(),
            "[remote \"origin\"]\n    url = https://github.com/user/repo.git\n",
        )?;

        let sub_dir = temp_dir.path().join("subdir");
        std::fs::create_dir(&sub_dir)?;
        create_git_config(
            &sub_dir,
            "[remote \"origin\"]\n    url = https://github.com/user/subrepo.git\n",
        )?;

        let mut cmd = Command::cargo_bin(get_binary_name())?;
        cmd.arg(temp_dir.path())
            .arg("-t")
            .assert()
            .success()
            .stdout(predicate::str::contains("https://github.com/user/repo.git"))
            .stdout(predicate::str::contains(
                "https://github.com/user/subrepo.git",
            ));

        Ok(())
    }

    #[test]
    fn test_cli_output_formats() -> Result<()> {
        let temp_dir = TempDir::new()?;
        create_git_config(
            temp_dir.path(),
            "[remote \"origin\"]\n    url = https://github.com/user/repo.git\n",
        )?;

        // Test plain format
        let mut cmd = Command::cargo_bin(get_binary_name())?;
        cmd.arg(temp_dir.path())
            .arg("-f")
            .arg("plain")
            .assert()
            .success()
            .stdout(predicate::str::contains("path:"))
            .stdout(predicate::str::contains("remotes:"))
            .stdout(predicate::str::contains(
                "origin: https://github.com/user/repo.git",
            ));

        // Test YAML format
        let mut cmd = Command::cargo_bin(get_binary_name())?;
        cmd.arg(temp_dir.path())
            .arg("-f")
            .arg("yaml")
            .assert()
            .success()
            .stdout(predicate::str::contains("path:"))
            .stdout(predicate::str::contains("remotes:"))
            .stdout(predicate::str::contains(
                "origin: https://github.com/user/repo.git",
            ));

        // Test JSON format
        let mut cmd = Command::cargo_bin(get_binary_name())?;
        cmd.arg(temp_dir.path())
            .arg("-f")
            .arg("json")
            .assert()
            .success()
            .stdout(predicate::str::contains("\"path\":"))
            .stdout(predicate::str::contains("\"remotes\":"))
            .stdout(predicate::str::contains(
                "\"origin\": \"https://github.com/user/repo.git\"",
            ));

        Ok(())
    }

    #[test]
    fn test_empty_directory() -> Result<()> {
        let temp_dir = TempDir::new()?;

        let mut cmd = Command::cargo_bin(get_binary_name())?;
        cmd.arg(temp_dir.path())
            .assert()
            .success()
            .stdout(predicate::eq(format!(
                "path: {}\n",
                temp_dir.path().display(),
            )));

        Ok(())
    }

    #[test]
    fn test_no_git_repositories() -> Result<()> {
        let temp_dir = TempDir::new()?;
        std::fs::create_dir(temp_dir.path().join("empty_dir"))?;

        let mut cmd = Command::cargo_bin(get_binary_name())?;
        cmd.arg(temp_dir.path())
            .arg("-t")
            .assert()
            .success()
            .stdout(predicate::eq(format!(
                "path: {}\n",
                temp_dir.path().display(),
            )));

        Ok(())
    }

    #[test]
    fn test_git_repo_no_remotes() -> Result<()> {
        let temp_dir = TempDir::new()?;
        create_git_config(temp_dir.path(), "")?;

        let mut cmd = Command::cargo_bin(get_binary_name())?;
        cmd.arg(temp_dir.path())
            .assert()
            .success()
            .stdout(predicate::str::contains("path:"))
            .stdout(predicate::str::contains("remotes:").count(0));

        Ok(())
    }

    #[test]
    fn test_git_repo_multiple_remotes() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let config_content = r#"
[remote "origin"]
    url = https://github.com/user/repo.git
[remote "upstream"]
    url = https://github.com/upstream/repo.git
"#;
        create_git_config(temp_dir.path(), config_content)?;

        let mut cmd = Command::cargo_bin(get_binary_name())?;
        cmd.arg(temp_dir.path())
            .assert()
            .success()
            .stdout(predicate::str::contains(
                "origin: https://github.com/user/repo.git",
            ))
            .stdout(predicate::str::contains(
                "upstream: https://github.com/upstream/repo.git",
            ));

        Ok(())
    }
}

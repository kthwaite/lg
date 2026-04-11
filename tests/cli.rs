use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::Result;
use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_lg"))
}

fn create_git_config(dir: &Path, content: &str) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir.join(".git"))?;
    let path = dir.join(".git/config");
    let mut file = File::create(path.clone())?;
    file.write_all(content.as_bytes())?;
    Ok(path)
}

#[test]
fn test_cli_valid_directory() -> Result<()> {
    let temp_dir = TempDir::new()?;
    create_git_config(
        temp_dir.path(),
        "[remote \"origin\"]\n    url = https://github.com/user/repo.git\n",
    )?;

    let mut cmd = cli();
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
    let mut cmd = cli();
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

    let mut cmd = cli();
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

    let mut cmd = cli();
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

    let mut cmd = cli();
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

    let mut cmd = cli();
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

    let mut cmd = cli();
    cmd.arg(temp_dir.path())
        .assert()
        .success()
        .stdout(predicate::eq(format!(
            "path: {}\n",
            temp_dir.path().display()
        )));

    Ok(())
}

#[test]
fn test_no_git_repositories() -> Result<()> {
    let temp_dir = TempDir::new()?;
    std::fs::create_dir(temp_dir.path().join("empty_dir"))?;

    let mut cmd = cli();
    cmd.arg(temp_dir.path())
        .arg("-t")
        .assert()
        .success()
        .stdout(predicate::eq(format!(
            "path: {}\n",
            temp_dir.path().display()
        )));

    Ok(())
}

#[test]
fn test_git_repo_no_remotes() -> Result<()> {
    let temp_dir = TempDir::new()?;
    create_git_config(temp_dir.path(), "")?;

    let mut cmd = cli();
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

    let mut cmd = cli();
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

//! Observe a checkout, plan the steps that bring it to the intended state,
//! then apply them. `plan` is pure so the decision table is unit-tested.

use crate::manifest::Target;
use std::error::Error;
use std::fmt;
use std::path::Path;
use std::process::Command;

#[derive(Debug, PartialEq)]
pub enum State {
    /// Nothing on disk (or an empty directory).
    Missing,
    /// A plain git repo; jj has not been initialised.
    GitOnly,
    Jj { colocated: bool, origin: Option<String> },
    /// Something is there that we must not touch.
    Blocked(String),
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Step {
    Init,
    Colocate,
    Remote,
    Fetch,
    /// Start a working-copy change on trunk (fresh clones only).
    Checkout,
}

impl fmt::Display for Step {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(match self {
            Step::Init => "init",
            Step::Colocate => "colocate",
            Step::Remote => "remote",
            Step::Fetch => "fetch",
            Step::Checkout => "checkout",
        })
    }
}

pub fn plan(state: &State, url: &str) -> Result<Vec<Step>, String> {
    Ok(match state {
        State::Missing => vec![Step::Init, Step::Remote, Step::Fetch, Step::Checkout],
        // Remote is idempotent, so it also covers the origin we cannot read yet.
        State::GitOnly => vec![Step::Init, Step::Remote, Step::Fetch],
        State::Jj { colocated, origin } => {
            let mut steps = vec![];
            if !colocated {
                steps.push(Step::Colocate);
            }
            if origin.as_deref() != Some(url) {
                steps.push(Step::Remote);
            }
            steps.push(Step::Fetch);
            steps
        }
        State::Blocked(why) => return Err(why.clone()),
    })
}

pub fn observe(path: &Path) -> Result<State, Box<dyn Error>> {
    if !path.exists() {
        return Ok(State::Missing);
    }
    if !path.is_dir() {
        return Ok(State::Blocked("path exists and is not a directory".into()));
    }
    if path.join(".jj").is_dir() {
        return Ok(State::Jj { colocated: path.join(".git").exists(), origin: origin(path)? });
    }
    if path.join(".git").exists() {
        return Ok(State::GitOnly);
    }
    if path.read_dir()?.next().is_none() {
        return Ok(State::Missing);
    }
    Ok(State::Blocked("directory is not empty and is not a repository".into()))
}

pub fn apply(target: &Target, steps: &[Step]) -> Result<(), Box<dyn Error>> {
    let dir = &target.path;
    for step in steps {
        match step {
            Step::Init => {
                std::fs::create_dir_all(dir)?;
                jj(dir, &["git", "init", "--colocate"])?;
            }
            Step::Colocate => {
                jj(dir, &["git", "colocation", "enable"])?;
            }
            Step::Remote => match origin(dir)? {
                Some(url) if url == target.url => {}
                Some(_) => {
                    jj(dir, &["git", "remote", "set-url", "origin", &target.url])?;
                }
                None => {
                    jj(dir, &["git", "remote", "add", "origin", &target.url])?;
                }
            },
            Step::Fetch => {
                jj(dir, &["git", "fetch", "--remote", "origin"])?;
            }
            Step::Checkout => {
                jj(dir, &["new", "trunk()"])?;
            }
        }
    }
    Ok(())
}

fn origin(dir: &Path) -> Result<Option<String>, Box<dyn Error>> {
    let out = jj(dir, &["git", "remote", "list"])?;
    Ok(out
        .lines()
        .find_map(|line| line.strip_prefix("origin "))
        .map(|url| url.trim().to_string()))
}

/// Run jj in `dir`; jj is chatty (every fetched tag), so output only surfaces on failure.
fn jj(dir: &Path, args: &[&str]) -> Result<String, Box<dyn Error>> {
    let out = Command::new("jj").args(args).current_dir(dir).output()?;
    if !out.status.success() {
        return Err(format!("jj {}: {}", args.join(" "), String::from_utf8_lossy(&out.stderr).trim()).into());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    const URL: &str = "https://h/o/r.git";

    fn jj_state(colocated: bool, origin: Option<&str>) -> State {
        State::Jj { colocated, origin: origin.map(String::from) }
    }

    #[test]
    fn missing_checkout_is_cloned() {
        assert_eq!(
            plan(&State::Missing, URL).unwrap(),
            [Step::Init, Step::Remote, Step::Fetch, Step::Checkout]
        );
    }

    #[test]
    fn git_only_repo_is_adopted_without_moving_its_checkout() {
        assert_eq!(plan(&State::GitOnly, URL).unwrap(), [Step::Init, Step::Remote, Step::Fetch]);
    }

    #[test]
    fn converged_repo_only_fetches() {
        assert_eq!(plan(&jj_state(true, Some(URL)), URL).unwrap(), [Step::Fetch]);
    }

    #[test]
    fn drift_is_corrected() {
        assert_eq!(
            plan(&jj_state(false, Some("https://other")), URL).unwrap(),
            [Step::Colocate, Step::Remote, Step::Fetch]
        );
        assert_eq!(plan(&jj_state(true, None), URL).unwrap(), [Step::Remote, Step::Fetch]);
    }

    #[test]
    fn blocked_path_is_an_error() {
        assert!(plan(&State::Blocked("x".into()), URL).is_err());
    }
}

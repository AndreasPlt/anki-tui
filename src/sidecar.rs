use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

const ANKI_VERSION: &str = "25.9.4";
const SIDECAR_ENV_ID: &str = "anki-25.9.4-v1";
const SIDECAR_SCRIPT_NAME: &str = "anki_tui_sidecar.py";
const SIDECAR_SCRIPT: &str = include_str!("../sidecar/anki_tui_sidecar.py");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Rating {
    Again = 1,
    Hard = 2,
    Good = 3,
    Easy = 4,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeckInfo {
    pub id: i64,
    pub name: String,
    pub new_count: u32,
    pub learn_count: u32,
    pub review_count: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReviewCounts {
    pub new: u32,
    pub learn: u32,
    pub review: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReviewButton {
    pub rating: u8,
    pub label: String,
    pub interval: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReviewCard {
    pub id: i64,
    pub question_html: String,
    pub answer_html: String,
    pub front_audio: Vec<String>,
    pub back_audio: Vec<String>,
    pub buttons: Vec<ReviewButton>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReviewSnapshot {
    pub deck_id: i64,
    pub deck_name: String,
    pub counts: ReviewCounts,
    pub card: Option<ReviewCard>,
}

#[derive(Debug, Deserialize)]
struct RpcResponse {
    id: u64,
    ok: bool,
    result: Option<Value>,
    error: Option<RpcError>,
}

#[derive(Debug, Deserialize)]
struct RpcError {
    code: String,
    message: String,
}

pub struct SidecarClient {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl SidecarClient {
    pub fn start(collection_path: &Path, media_dir: &Path) -> Result<Self> {
        let mut child = spawn_sidecar()?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| Error::Sidecar("failed to open sidecar stdin".to_string()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| Error::Sidecar("failed to open sidecar stdout".to_string()))?;
        let mut client = Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            next_id: 1,
        };
        client.open_collection(collection_path, media_dir)?;
        Ok(client)
    }

    pub fn open_collection(&mut self, collection_path: &Path, media_dir: &Path) -> Result<()> {
        let _: Value = self.call(
            "open_collection",
            json!({
                "collection_path": collection_path.to_string_lossy(),
                "media_dir": media_dir.to_string_lossy(),
            }),
        )?;
        Ok(())
    }

    pub fn list_decks(&mut self) -> Result<Vec<DeckInfo>> {
        #[derive(Deserialize)]
        struct ListDecks {
            decks: Vec<DeckInfo>,
        }
        Ok(self.call::<ListDecks>("list_decks", json!({}))?.decks)
    }

    pub fn start_review(&mut self, deck_id: i64, dry_run: bool) -> Result<ReviewSnapshot> {
        self.call(
            "start_review",
            json!({
                "deck_id": deck_id,
                "dry_run": dry_run,
            }),
        )
    }

    pub fn answer_card(&mut self, card_id: i64, rating: Rating) -> Result<ReviewSnapshot> {
        self.call(
            "answer_card",
            json!({
                "card_id": card_id,
                "rating": rating as u8,
            }),
        )
    }

    fn call<T: for<'de> Deserialize<'de>>(&mut self, method: &str, params: Value) -> Result<T> {
        let id = self.next_id;
        self.next_id += 1;
        let request = json!({
            "id": id,
            "method": method,
            "params": params,
        });
        writeln!(self.stdin, "{request}")?;
        self.stdin.flush()?;

        let response = loop {
            let mut line = String::new();
            let read = self.stdout.read_line(&mut line)?;
            if read == 0 {
                return Err(Error::SidecarProtocol(
                    "sidecar exited before sending a response".to_string(),
                ));
            }
            match serde_json::from_str::<RpcResponse>(&line) {
                Ok(response) => break response,
                Err(_) if !line.trim_start().starts_with('{') => continue,
                Err(e) => {
                    return Err(Error::SidecarProtocol(format!(
                        "invalid JSON response: {e}: {line}"
                    )));
                }
            }
        };
        if response.id != id {
            return Err(Error::SidecarProtocol(format!(
                "response id mismatch: expected {id}, got {}",
                response.id
            )));
        }
        if !response.ok {
            let error = response
                .error
                .map(|e| format!("{}: {}", e.code, e.message))
                .unwrap_or_else(|| "unknown sidecar error".to_string());
            return Err(Error::Sidecar(error));
        }
        let result = response
            .result
            .ok_or_else(|| Error::SidecarProtocol("missing result".to_string()))?;
        serde_json::from_value(result)
            .map_err(|e| Error::SidecarProtocol(format!("invalid result for {method}: {e}")))
    }
}

impl Drop for SidecarClient {
    fn drop(&mut self) {
        let id = self.next_id;
        let request = json!({
            "id": id,
            "method": "shutdown",
            "params": {},
        });
        let _ = writeln!(self.stdin, "{request}");
        let _ = self.stdin.flush();
        let _ = self.child.wait();
    }
}

fn spawn_sidecar() -> Result<Child> {
    if let Some(child) = spawn_custom_sidecar()? {
        return Ok(child);
    }

    let (python, script) = prepare_managed_sidecar()?;
    Command::new(python)
        .arg("-I")
        .arg("-u")
        .arg(script)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| Error::Sidecar(format!("failed to spawn managed sidecar: {e}")))
}

fn spawn_custom_sidecar() -> Result<Option<Child>> {
    let Some(cmd) = env::var_os("ANKI_TUI_SIDECAR_CMD") else {
        return Ok(None);
    };
    let cmd = cmd.to_string_lossy();
    let mut parts = cmd.split_whitespace();
    let executable = parts
        .next()
        .ok_or_else(|| Error::Sidecar("ANKI_TUI_SIDECAR_CMD is empty".to_string()))?;
    Command::new(executable)
        .args(parts)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map(Some)
        .map_err(|e| Error::Sidecar(format!("failed to spawn sidecar command: {e}")))
}

fn prepare_managed_sidecar() -> Result<(PathBuf, PathBuf)> {
    let home = sidecar_home()?;
    fs::create_dir_all(&home)?;

    let script = home.join(SIDECAR_SCRIPT_NAME);
    write_if_changed(&script, SIDECAR_SCRIPT.as_bytes())?;

    let venv = home.join("venvs").join(SIDECAR_ENV_ID);
    let venv_python = venv_python_path(&venv);
    let marker = venv.join(".anki-tui-ready");
    if venv_python.exists() && marker_contents_match(&marker, SIDECAR_ENV_ID)? {
        return Ok((venv_python, script));
    }

    let wheelhouse = wheelhouse_dir()?;
    let python = find_python()?;

    if venv.exists() {
        fs::remove_dir_all(&venv).map_err(|e| {
            Error::Sidecar(format!(
                "failed to remove stale sidecar environment at {}: {e}",
                venv.display()
            ))
        })?;
    }
    if let Some(parent) = venv.parent() {
        fs::create_dir_all(parent)?;
    }

    run_checked(
        Command::new(&python).arg("-I").arg("-m").arg("venv").arg(&venv),
        "create sidecar Python environment",
    )?;
    let mut pip = Command::new(&venv_python);
    pip.arg("-I").arg("-m").arg("pip").arg("install");
    let description = if let Some(wheelhouse) = &wheelhouse {
        pip.arg("--no-index").arg("--find-links").arg(wheelhouse);
        "install sidecar Python dependencies from wheelhouse"
    } else {
        eprintln!("anki-tui: no local wheelhouse found; downloading anki=={ANKI_VERSION} from PyPI (one-time setup)");
        "install sidecar Python dependencies from PyPI"
    };
    pip.arg(format!("anki=={ANKI_VERSION}"));
    run_checked(&mut pip, description)?;
    fs::write(&marker, SIDECAR_ENV_ID)?;

    Ok((venv_python, script))
}

fn sidecar_home() -> Result<PathBuf> {
    if let Some(path) = non_empty_env_path("ANKI_TUI_SIDECAR_HOME") {
        return Ok(path);
    }
    data_home()
        .map(|path| path.join("anki-tui").join("sidecar"))
        .ok_or_else(|| {
            Error::Sidecar(
                "could not determine sidecar home; set ANKI_TUI_SIDECAR_HOME or HOME".to_string(),
            )
        })
}

fn wheelhouse_dir() -> Result<Option<PathBuf>> {
    if let Some(path) = non_empty_env_path("ANKI_TUI_WHEELHOUSE_DIR") {
        let path = existing_wheelhouse(path, "ANKI_TUI_WHEELHOUSE_DIR")?;
        ensure_wheelhouse_contains_anki(&path)?;
        return Ok(Some(path));
    }

    let mut candidates = Vec::new();
    if let Some(xdg_data_home) = non_empty_env_path("XDG_DATA_HOME") {
        candidates.push(xdg_data_home.join("anki-tui").join("wheels"));
    }
    if let Some(home_data_dir) = home_data_dir() {
        let candidate = home_data_dir.join("anki-tui").join("wheels");
        if !candidates.contains(&candidate) {
            candidates.push(candidate);
        }
    }
    if let Ok(exe) = env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            candidates.push(
                exe_dir
                    .join("..")
                    .join("share")
                    .join("anki-tui")
                    .join("wheels"),
            );
        }
    }
    candidates.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("sidecar")
            .join("wheels"),
    );

    Ok(select_wheelhouse(candidates))
}

fn select_wheelhouse(candidates: Vec<PathBuf>) -> Option<PathBuf> {
    candidates
        .into_iter()
        .find(|path| path.is_dir() && ensure_wheelhouse_contains_anki(path).is_ok())
}

fn existing_wheelhouse(path: PathBuf, source: &str) -> Result<PathBuf> {
    if path.is_dir() {
        Ok(path)
    } else {
        Err(Error::Sidecar(format!(
            "{source} does not point to an existing wheelhouse directory: {}",
            path.display()
        )))
    }
}

pub(crate) fn data_home() -> Option<PathBuf> {
    let xdg_data_home = env::var_os("XDG_DATA_HOME");
    let home = env::var_os("HOME");
    data_home_from_values(xdg_data_home.as_deref(), home.as_deref())
}

fn data_home_from_values(xdg_data_home: Option<&OsStr>, home: Option<&OsStr>) -> Option<PathBuf> {
    env_path_from_value(xdg_data_home).or_else(|| home_data_dir_from_value(home))
}

fn home_data_dir() -> Option<PathBuf> {
    let home = env::var_os("HOME");
    home_data_dir_from_value(home.as_deref())
}

fn home_data_dir_from_value(home: Option<&OsStr>) -> Option<PathBuf> {
    env_path_from_value(home).map(|home| home.join(".local").join("share"))
}

fn non_empty_env_path(name: &str) -> Option<PathBuf> {
    let value = env::var_os(name);
    env_path_from_value(value.as_deref())
}

fn env_path_from_value(value: Option<&OsStr>) -> Option<PathBuf> {
    value.and_then(|value| {
        if value.is_empty() {
            None
        } else {
            Some(PathBuf::from(value))
        }
    })
}

fn find_python() -> Result<PathBuf> {
    if let Some(path) = non_empty_env_path("ANKI_TUI_PYTHON") {
        let version = python_version(&path)?;
        if is_supported_python(version) {
            return Ok(path);
        }
        return Err(unsupported_python_error(&path, version));
    }

    for candidate in [
        "python3.13",
        "python3.12",
        "python3.11",
        "python3.10",
        "python3",
    ] {
        let path = PathBuf::from(candidate);
        if let Ok(version) = python_version(&path) {
            if is_supported_python(version) {
                return Ok(path);
            }
        }
    }

    Err(Error::Sidecar(
        "could not find Python 3.10, 3.11, 3.12, or 3.13; set ANKI_TUI_PYTHON".to_string(),
    ))
}

fn python_version(python: &Path) -> Result<(u32, u32)> {
    let output = Command::new(python)
        .arg("--version")
        .output()
        .map_err(|e| {
            Error::Sidecar(format!("failed to run {} --version: {e}", python.display()))
        })?;
    if !output.status.success() {
        return Err(Error::Sidecar(format!(
            "{} --version failed with status {}",
            python.display(),
            output.status
        )));
    }
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    parse_python_version(&text).ok_or_else(|| {
        Error::Sidecar(format!(
            "could not parse Python version from {}: {text:?}",
            python.display()
        ))
    })
}

fn parse_python_version(text: &str) -> Option<(u32, u32)> {
    for token in text.split(|ch: char| !(ch.is_ascii_digit() || ch == '.')) {
        if token.is_empty() {
            continue;
        }
        let mut parts = token.split('.');
        let Some(major) = parts.next().and_then(|part| part.parse().ok()) else {
            continue;
        };
        let Some(minor) = parts.next().and_then(|part| part.parse().ok()) else {
            continue;
        };
        return Some((major, minor));
    }
    None
}

fn is_supported_python((major, minor): (u32, u32)) -> bool {
    major == 3 && (10..14).contains(&minor)
}

fn unsupported_python_error(python: &Path, version: (u32, u32)) -> Error {
    Error::Sidecar(format!(
        "{} is Python {}.{}, but anki-tui requires Python >=3.10,<3.14 for the sidecar",
        python.display(),
        version.0,
        version.1
    ))
}

fn venv_python_path(venv: &Path) -> PathBuf {
    if cfg!(windows) {
        venv.join("Scripts").join("python.exe")
    } else {
        venv.join("bin").join("python")
    }
}

fn run_checked(command: &mut Command, description: &str) -> Result<()> {
    let status = command
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|e| Error::Sidecar(format!("failed to {description}: {e}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(Error::Sidecar(format!(
            "failed to {description}; command exited with {status}"
        )))
    }
}

fn write_if_changed(path: &Path, contents: &[u8]) -> Result<()> {
    if fs::read(path).is_ok_and(|existing| existing == contents) {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, contents)?;
    Ok(())
}

fn marker_contents_match(path: &Path, expected: &str) -> Result<bool> {
    match fs::read_to_string(path) {
        Ok(contents) => Ok(contents == expected),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.into()),
    }
}

fn ensure_wheelhouse_contains_anki(wheelhouse: &Path) -> Result<()> {
    let expected_prefix = format!("anki-{ANKI_VERSION}");
    let contains_anki = fs::read_dir(wheelhouse)?.any(|entry| {
        entry
            .ok()
            .and_then(|entry| entry.file_name().into_string().ok())
            .is_some_and(|name| name.starts_with(&expected_prefix) && name.ends_with(".whl"))
    });
    if contains_anki {
        Ok(())
    } else {
        Err(Error::Sidecar(format!(
            "sidecar wheelhouse at {} does not contain anki=={ANKI_VERSION}; run scripts/build-sidecar-wheelhouse.sh",
            wheelhouse.display()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ANKI_VERSION, data_home_from_values, ensure_wheelhouse_contains_anki, is_supported_python,
        parse_python_version, select_wheelhouse, venv_python_path,
    };
    use std::ffi::OsStr;
    use std::fs;
    use std::path::{Path, PathBuf};

    #[test]
    fn parses_python_version_output() {
        assert_eq!(parse_python_version("Python 3.13.2\n"), Some((3, 13)));
        assert_eq!(parse_python_version("Python 3.10.14"), Some((3, 10)));
    }

    #[test]
    fn validates_supported_python_range() {
        assert!(!is_supported_python((3, 9)));
        assert!(is_supported_python((3, 10)));
        assert!(is_supported_python((3, 13)));
        assert!(!is_supported_python((3, 14)));
        assert!(!is_supported_python((2, 7)));
    }

    #[test]
    fn builds_platform_venv_python_path() {
        let path = venv_python_path(Path::new("/tmp/anki-tui-venv"));
        if cfg!(windows) {
            assert!(path.ends_with("Scripts/python.exe"));
        } else {
            assert!(path.ends_with("bin/python"));
        }
    }

    #[test]
    fn resolves_data_home_with_home_fallback() {
        assert_eq!(
            data_home_from_values(
                Some(OsStr::new("/xdg/data")),
                Some(OsStr::new("/home/user"))
            ),
            Some(PathBuf::from("/xdg/data"))
        );
        assert_eq!(
            data_home_from_values(None, Some(OsStr::new("/home/user"))),
            Some(PathBuf::from("/home/user/.local/share"))
        );
        assert_eq!(
            data_home_from_values(Some(OsStr::new("")), Some(OsStr::new("/home/user"))),
            Some(PathBuf::from("/home/user/.local/share"))
        );
        assert_eq!(data_home_from_values(None, None), None);
    }

    #[test]
    fn validates_wheelhouse_contains_pinned_anki_wheel() {
        let wheelhouse = unique_temp_dir("anki-tui-wheelhouse");
        let _ = fs::remove_dir_all(&wheelhouse);
        fs::create_dir_all(&wheelhouse).unwrap();

        assert!(ensure_wheelhouse_contains_anki(&wheelhouse).is_err());

        fs::write(
            wheelhouse.join("anki-25.9.4-py3-none-any.whl"),
            b"not a real wheel",
        )
        .unwrap();
        assert!(ensure_wheelhouse_contains_anki(&wheelhouse).is_ok());

        fs::remove_dir_all(wheelhouse).unwrap();
    }

    #[test]
    fn selects_first_candidate_with_pinned_wheel_or_falls_back() {
        let missing = unique_temp_dir("anki-tui-wh-missing");
        let _ = fs::remove_dir_all(&missing);

        let stale = unique_temp_dir("anki-tui-wh-stale");
        let _ = fs::remove_dir_all(&stale);
        fs::create_dir_all(&stale).unwrap();
        fs::write(stale.join("anki-0.0.1-py3-none-any.whl"), b"stale wheel").unwrap();

        let valid = unique_temp_dir("anki-tui-wh-valid");
        let _ = fs::remove_dir_all(&valid);
        fs::create_dir_all(&valid).unwrap();
        fs::write(
            valid.join(format!("anki-{ANKI_VERSION}-py3-none-any.whl")),
            b"not a real wheel",
        )
        .unwrap();

        // Missing and stale candidates are skipped in favor of the valid one.
        assert_eq!(
            select_wheelhouse(vec![missing.clone(), stale.clone(), valid.clone()]),
            Some(valid.clone())
        );
        // No valid candidate -> None, which triggers the PyPI fallback.
        assert_eq!(select_wheelhouse(vec![missing, stale.clone()]), None);
        assert_eq!(select_wheelhouse(Vec::new()), None);

        fs::remove_dir_all(stale).unwrap();
        fs::remove_dir_all(valid).unwrap();
    }

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        std::env::temp_dir().join(format!("{prefix}-{}", std::process::id()))
    }
}

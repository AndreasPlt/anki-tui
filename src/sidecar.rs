use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

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
    #[allow(dead_code)]
    pub front_audio: Vec<String>,
    #[allow(dead_code)]
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
    if let Ok(cmd) = std::env::var("ANKI_TUI_SIDECAR_CMD") {
        let mut parts = cmd.split_whitespace();
        let executable = parts
            .next()
            .ok_or_else(|| Error::Sidecar("ANKI_TUI_SIDECAR_CMD is empty".to_string()))?;
        return Command::new(executable)
            .args(parts)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| Error::Sidecar(format!("failed to spawn sidecar command: {e}")));
    }

    let uv = std::env::var("ANKI_TUI_UV").unwrap_or_else(|_| "uv".to_string());
    let sidecar_dir = sidecar_dir();
    let script = sidecar_dir.join("anki_tui_sidecar.py");
    Command::new(uv)
        .arg("--project")
        .arg(&sidecar_dir)
        .arg("run")
        .arg("python")
        .arg("-u")
        .arg(script)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| Error::Sidecar(format!("failed to spawn uv sidecar: {e}")))
}

fn sidecar_dir() -> PathBuf {
    std::env::var_os("ANKI_TUI_SIDECAR_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("sidecar"))
}

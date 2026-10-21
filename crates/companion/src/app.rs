//! `film-lab app`: the automatic mode inside PhotoCraft.
//!
//! A plug-in band never sees the whole layer, so the whole-image analysis happens here: the
//! companion connects to the running app (`photocraft --control PORT --control-token-file F
//! --automation-read-root DIR --automation-write-root DIR`), saves a flattened PNG copy of the
//! open document through the control channel, analyses it, installs the plug-in from bytes and
//! either runs it with the analysis (one history step, nothing to type) or opens its dialog with
//! every value filled in, for adjusting with the live preview before OK.

use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Map, Value, json};

use crate::{AnalyzeOptions, Levels, Plugin, analyze};

/// PhotoCraft's default control port.
pub const DEFAULT_ADDR: &str = "127.0.0.1:7878";

#[derive(Clone, Debug)]
pub struct AppOptions {
    pub addr: String,
    pub token: String,
    /// The app's automation write root: where it saves the copy this tool reads back.
    pub root: PathBuf,
    /// Open the plug-in's dialog with the analysis filled in instead of converting outright.
    pub dialog: bool,
    pub analyze: AnalyzeOptions,
    /// Plug-in parameters on top of the analysis.
    pub set: Map<String, Value>,
    /// Which plug-in to install and run, and its module.
    pub plugin: Plugin,
    pub wasm: PathBuf,
}

/// A JSON-lines connection to PhotoCraft's control channel, authenticated.
pub struct Control {
    reader: BufReader<TcpStream>,
    writer: TcpStream,
    next_id: u64,
}

impl Control {
    pub fn connect(addr: &str, token: &str) -> Result<Self, String> {
        let mut last = format!("cannot resolve {addr}");
        let mut stream = None;
        for sa in addr.to_socket_addrs().map_err(|e| format!("{addr}: {e}"))? {
            match TcpStream::connect_timeout(&sa, Duration::from_millis(1500)) {
                Ok(s) => {
                    stream = Some(s);
                    break;
                }
                Err(e) => last = e.to_string(),
            }
        }
        let Some(s) = stream else {
            return Err(format!("PhotoCraft is not reachable at {addr}: {last} (start it with `photocraft --control PORT …`)"));
        };
        s.set_nodelay(true).ok();
        s.set_read_timeout(Some(Duration::from_secs(90))).ok();
        let reader = BufReader::new(s.try_clone().map_err(|e| e.to_string())?);
        let mut c = Self { reader, writer: s, next_id: 1 };
        c.call("auth", json!({"token": token})).map_err(|e| format!("authentication refused: {e}"))?;
        Ok(c)
    }

    /// One control method; `Ok` is its result.
    pub fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next_id;
        self.next_id += 1;
        let line = json!({"id": id, "method": method, "params": params}).to_string();
        self.writer.write_all(line.as_bytes()).and_then(|()| self.writer.write_all(b"\n")).map_err(|e| format!("{method}: {e}"))?;
        let mut reply = String::new();
        if self.reader.read_line(&mut reply).map_err(|e| format!("{method}: {e}"))? == 0 {
            return Err(format!("{method}: PhotoCraft closed the connection"));
        }
        let v: Value = serde_json::from_str(reply.trim()).map_err(|e| format!("{method}: bad reply: {e}"))?;
        if v.get("ok").and_then(Value::as_bool) == Some(true) {
            Ok(v.get("result").cloned().unwrap_or(Value::Null))
        } else {
            Err(format!("{method}: {}", v.get("error").and_then(Value::as_str).unwrap_or("unknown error")))
        }
    }

    /// An engine or UI command by id.
    pub fn execute(&mut self, command: &str, params: Value) -> Result<Value, String> {
        self.call("engine.execute", json!({"command": command, "params": params}))
    }
}

/// Measures the document open in PhotoCraft from a flattened copy saved through the app's
/// automation write root `root` (analysed, then deleted): the pixels the plug-in will see,
/// whatever the app did when it opened the file (a colour profile converted to the working space,
/// a format decoded its own way), which a measurement of the file on disk can miss.
pub fn measure_open_document(c: &mut Control, root: &Path, o: &AnalyzeOptions) -> Result<Levels, String> {
    let state = c.call("ui.inspect", json!({}))?;
    if state.get("document").is_none_or(Value::is_null) {
        return Err("no document is open in PhotoCraft: open the scan first".into());
    }
    let name = format!("film-lab-{}.png", std::process::id());
    c.execute("image.duplicate", json!({}))?;
    let saved = c.call("app.save", json!({"path": name}));
    let closed = c.execute("file.close", json!({}));
    if let Some(prompt) = c.call("ui.inspect", json!({}))?.get("dialogs").and_then(Value::as_array).and_then(|d| d.first())
        && prompt.get("fields").and_then(|f| f.get("command")).is_some()
    {
        c.call("ui.dialog.confirm", json!({"dialog": "discard", "choice": "dontSave"})).ok();
    }
    saved?;
    closed?;
    let path = root.join(&name);
    let img = image::open(&path)
        .map_err(|e| format!("cannot read the copy PhotoCraft saved at {}: {e} (is --root the app's automation write root?)", path.display()));
    std::fs::remove_file(&path).ok();
    analyze(&img?, o)
}

/// Converts the document open in PhotoCraft (or opens the plug-in's dialog filled in).
pub fn convert_in_app(o: &AppOptions) -> Result<Value, String> {
    let mut c = Control::connect(&o.addr, &o.token)?;
    let levels = measure_open_document(&mut c, &o.root, &o.analyze)?;
    let mut params = levels.plugin_params();
    params.extend(o.set.clone());

    // The measurement travels inside the module, so the dialog never shows it.
    let bytes = std::fs::read(&o.wasm).map_err(|e| format!("cannot read the plug-in {}: {e}", o.wasm.display()))?;
    let module = levels.embed(&bytes)?;
    c.execute("plugin.install", json!({"data": base64(&module), "replace": true}))?;
    let measurement = levels.measurement();
    if o.dialog {
        let r = c.call("ui.menu.invoke", json!({"id": format!("plugin.filter.{}", o.plugin.id())}))?;
        let dialog = r.get("dialog").cloned().ok_or("the plug-in's dialog did not open")?;
        for (k, v) in &params {
            c.call("ui.dialog.set", json!({"dialog": dialog, "field": k, "value": v}))?;
        }
        Ok(json!({"dialog": dialog, "samples": levels.samples, "measurement": measurement, "params": params}))
    } else {
        let r = c.execute("plugin.run", json!({"id": o.plugin.id(), "params": params}))?;
        Ok(json!({"applied": r, "samples": levels.samples, "measurement": measurement, "params": params}))
    }
}

/// Standard base64 (the plug-in module goes over the channel as bytes).
pub fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk.first().copied().unwrap_or(0), chunk.get(1).copied().unwrap_or(0), chunk.get(2).copied().unwrap_or(0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        let sextets = [(n >> 18) & 63, (n >> 12) & 63, (n >> 6) & 63, n & 63];
        for (i, s) in sextets.iter().enumerate() {
            if i <= chunk.len() {
                out.push(char::from(ALPHABET[*s as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// The control token: `--token`, `--token-file`, or the environment PhotoCraft itself honours.
pub fn find_token(token: Option<&str>, file: Option<&std::path::Path>) -> Result<String, String> {
    if let Some(t) = token {
        return Ok(t.trim().to_string());
    }
    if let Some(f) = file {
        return std::fs::read_to_string(f).map(|s| s.trim().to_string()).map_err(|e| format!("cannot read the token file {}: {e}", f.display()));
    }
    if let Ok(t) = std::env::var("PHOTOCRAFT_CONTROL_TOKEN") {
        return Ok(t.trim().to_string());
    }
    if let Ok(f) = std::env::var("PHOTOCRAFT_CONTROL_TOKEN_FILE") {
        return std::fs::read_to_string(&f).map(|s| s.trim().to_string()).map_err(|e| format!("cannot read the token file {f}: {e}"));
    }
    Err("no control token: pass --token-file (the file given to `photocraft --control-token-file`) or --token".into())
}

/// Where PhotoCraft is: `explicit`, `$PHOTOCRAFT`, the sibling checkout's release build next to
/// this repository, then the installed app.
pub fn find_photocraft(explicit: Option<&Path>) -> Result<PathBuf, String> {
    if let Some(p) = explicit {
        return Ok(p.to_path_buf());
    }
    if let Some(p) = std::env::var_os("PHOTOCRAFT").filter(|p| !p.is_empty()) {
        return Ok(PathBuf::from(p));
    }
    let candidates = [
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../photocraft/target/release/photocraft"),
        PathBuf::from("/Applications/PhotoCraft.app/Contents/MacOS/PhotoCraft"),
    ];
    candidates.into_iter().find(|p| p.is_file()).ok_or_else(|| "no PhotoCraft binary: pass --photocraft or set $PHOTOCRAFT".into())
}

/// PhotoCraft started by [`launch`]: the process, its control channel's address and token.
pub struct Launched {
    pub child: Child,
    pub addr: String,
    pub token: String,
}

/// Starts `photocraft` with its control channel on `port`, `work` as both its automation roots
/// and its token file and log inside it (`token`, `photocraft.log`), and waits for the channel
/// to answer. Kill the child to quit the app: `file.exit` is refused over automation.
pub fn launch(photocraft: &Path, work: &Path, port: u16) -> Result<Launched, String> {
    std::fs::create_dir_all(work).map_err(|e| format!("cannot create {}: {e}", work.display()))?;
    let work = work.canonicalize().map_err(|e| e.to_string())?;
    let token_file = work.join("token");
    std::fs::remove_file(&token_file).ok();
    let log_path = work.join("photocraft.log");
    let log = std::fs::File::create(&log_path).map_err(|e| e.to_string())?;
    let mut child = Command::new(photocraft)
        .args(["--control", &port.to_string(), "--control-token-file"])
        .arg(&token_file)
        .arg("--automation-read-root")
        .arg(&work)
        .arg("--automation-write-root")
        .arg(&work)
        .stdout(Stdio::from(log.try_clone().map_err(|e| e.to_string())?))
        .stderr(Stdio::from(log))
        .spawn()
        .map_err(|e| format!("cannot start {}: {e}", photocraft.display()))?;
    let addr = format!("127.0.0.1:{port}");
    let sa: SocketAddr = addr.parse().map_err(|e| format!("{addr}: {e}"))?;
    let started = Instant::now();
    while TcpStream::connect_timeout(&sa, Duration::from_millis(300)).is_err() || !token_file.is_file() {
        if let Ok(Some(status)) = child.try_wait() {
            return Err(format!("PhotoCraft exited ({status}) before opening its control channel; see {}", log_path.display()));
        }
        if started.elapsed() > Duration::from_secs(90) {
            child.kill().ok();
            return Err(format!("PhotoCraft did not open its control channel within 90 s; see {}", log_path.display()));
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    // The channel answers before the window is ready to take commands.
    std::thread::sleep(Duration::from_secs(2));
    let token = find_token(None, Some(&token_file))?;
    Ok(Launched { child, addr, token })
}

#[cfg(test)]
mod tests {
    use super::base64;

    #[test]
    fn base64_matches_the_standard() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64(&[0, 255, 254]), "AP/+");
    }
}

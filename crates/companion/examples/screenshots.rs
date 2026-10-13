//! Regenerates the README's screenshots and the flow GIF by driving PhotoCraft.
//!
//! ```sh
//! scripts/screenshots.sh                      # builds, fetches the sample, runs this
//! cargo run -p film-lab-companion --example screenshots -- --photocraft /path/to/photocraft
//! ```
//!
//! Launches PhotoCraft with its control channel on a work folder, opens the sample negative,
//! has the companion measure it and install the plug-in with the measurement, finds Film Lab in
//! the command search, opens its dialog from there, confirms, and screenshots each step; then
//! writes `docs/images/*.jpg` and `docs/images/flow.gif`.
//! Options: `--photocraft BIN` (or `$PHOTOCRAFT`; default: `../photocraft/target/release/photocraft`
//! next to this repository, then the installed app), `--negative FILE` (default: the Ektar frame
//! from `scripts/fetch-samples.sh`), `--out DIR` (default `docs/images`), `--work DIR` (default
//! `target/screenshots`), `--port N` (default 7878).

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use film_lab_companion::app::{AppOptions, Control, convert_in_app, find_photocraft, launch};
use film_lab_companion::{AnalyzeOptions, WbMode, find_plugin};
use image::codecs::gif::{GifEncoder, Repeat};
use image::{Delay, Frame, ImageReader, imageops::FilterType};
use serde_json::{Map, json};

/// One captured step, in order: the screenshot's name (its JPEG in `docs/images` has the same
/// name) and how long the GIF shows it.
struct Step {
    png: &'static str,
    gif_ms: u32,
}

const STEPS: [Step; 4] = [
    Step { png: "01-negative.png", gif_ms: 1800 },
    Step { png: "02-search.png", gif_ms: 2000 },
    Step { png: "03-dialog.png", gif_ms: 2600 },
    Step { png: "04-converted.png", gif_ms: 3000 },
];

struct Args {
    photocraft: PathBuf,
    negative: PathBuf,
    out: PathBuf,
    work: PathBuf,
    port: u16,
}

fn main() {
    if let Err(e) = run() {
        eprintln!("screenshots: {e}");
        std::process::exit(1);
    }
}

fn parse() -> Result<Args, String> {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut a = Args {
        photocraft: PathBuf::new(),
        negative: repo.join("corpus/ektar-frame22-negative.png"),
        out: repo.join("docs/images"),
        work: repo.join("target/screenshots"),
        port: 7878,
    };
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while let Some(arg) = args.get(i) {
        let value = |i: &mut usize| -> Result<&String, String> {
            *i += 1;
            args.get(*i).ok_or_else(|| format!("{arg} expects a value"))
        };
        match arg.as_str() {
            "--photocraft" => a.photocraft = PathBuf::from(value(&mut i)?),
            "--negative" => a.negative = PathBuf::from(value(&mut i)?),
            "--out" => a.out = PathBuf::from(value(&mut i)?),
            "--work" => a.work = PathBuf::from(value(&mut i)?),
            "--port" => a.port = value(&mut i)?.parse().map_err(|_| "--port expects a number")?,
            other => return Err(format!("unknown option {other}")),
        }
        i += 1;
    }
    a.photocraft = find_photocraft(if a.photocraft.as_os_str().is_empty() { None } else { Some(&a.photocraft) })?;
    Ok(a)
}

fn run() -> Result<(), String> {
    let a = parse()?;
    if !a.negative.is_file() {
        return Err(format!("{} is missing: run scripts/fetch-samples.sh", a.negative.display()));
    }
    std::fs::create_dir_all(&a.work).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&a.out).map_err(|e| e.to_string())?;
    let work = a.work.canonicalize().map_err(|e| e.to_string())?;
    std::fs::copy(&a.negative, work.join("negative.png")).map_err(|e| format!("cannot copy the negative: {e}"))?;

    // PhotoCraft on its control channel, with the work folder as its automation roots.
    let mut app = launch(&a.photocraft, &work, a.port)?;
    let result = drive(&a, &work, &app.addr, &app.token);
    app.child.kill().ok();
    app.child.wait().ok();
    result
}

fn drive(a: &Args, work: &Path, addr: &str, token: &str) -> Result<(), String> {
    let mut c = Control::connect(addr, token)?;
    let settle = |ms: u64| std::thread::sleep(Duration::from_millis(ms));
    let shot = |c: &mut Control, name: &str| -> Result<(), String> {
        settle(1200);
        c.call("ui.screenshot", json!({"path": name}))?;
        println!("captured {name}");
        Ok(())
    };

    // A clean window: no floating Properties panel, no other document.
    c.call("ui.set", json!({"panels": {"properties": false}})).ok();
    for _ in 0..3 {
        if c.call("ui.inspect", json!({}))?.get("document").is_none_or(serde_json::Value::is_null) {
            break;
        }
        c.execute("file.close", json!({})).ok();
        if c.call("ui.inspect", json!({}))?.get("dialogs").and_then(|d| d.as_array()).is_some_and(|d| !d.is_empty()) {
            c.call("ui.dialog.confirm", json!({"dialog": "discard", "choice": "dontSave"})).ok();
        }
    }

    // 1. The negative.
    c.call("app.open", json!({"path": "negative.png"}))?;
    c.call("ui.set", json!({"tool": "move"})).ok();
    c.call("ui.set", json!({"fit": true}))?;
    shot(&mut c, "01-negative.png")?;

    // 2. The companion measures the document and installs the plug-in with the measurement
    // inside. Its dialog option opens the dialog straight away; here that dialog is cancelled
    // so the next steps show the menu route to the same dialog.
    let o = AppOptions {
        addr: addr.to_string(),
        token: token.to_string(),
        root: work.to_path_buf(),
        dialog: true,
        analyze: AnalyzeOptions { border_percent: 8.0, wb: WbMode::Neutral, ..AnalyzeOptions::default() },
        set: Map::new(),
        plugin: find_plugin(None)?,
    };
    let r = convert_in_app(&o)?;
    if let Some(d) = r.get("dialog") {
        c.call("ui.dialog.cancel", json!({"dialog": d}))?;
    }
    settle(800);

    // 3. Once installed, the command search (⌘F) finds Film Lab… under Filter › Plug-ins; Enter
    // opens its dialog, which carries the companion's measurement.
    c.call("ui.menu.invoke", json!({"id": "edit.search"}))?;
    settle(500);
    c.call("ui.type", json!({"text": "film lab"}))?;
    shot(&mut c, "02-search.png")?;
    c.call("ui.key", json!({"key": "Enter"}))?;
    settle(1200);
    let open_dialog = |c: &mut Control| -> Result<Option<serde_json::Value>, String> {
        let state = c.call("ui.inspect", json!({}))?;
        let dialogs = state.get("dialogs").and_then(|d| d.as_array()).cloned().unwrap_or_default();
        Ok(dialogs.iter().find(|d| d.get("fields").and_then(|f| f.get("__command")) == Some(&json!("plugin.run"))).and_then(|d| d.get("id")).cloned())
    };
    let dialog = match open_dialog(&mut c)? {
        Some(d) => d,
        None => {
            // The search didn't run it (a different top result): the menu entry does.
            eprintln!("note: the search did not open Film Lab; opening it from the Filter menu instead");
            c.call("ui.menu.invoke", json!({"id": format!("plugin.filter.{}", film_lab_companion::PLUGIN_ID)}))?;
            settle(1000);
            open_dialog(&mut c)?.ok_or("Film Lab's dialog did not open")?
        }
    };
    settle(2000);
    shot(&mut c, "03-dialog.png")?;

    // 4. OK.
    c.call("ui.dialog.confirm", json!({"dialog": dialog}))?;
    c.call("ui.set", json!({"fit": true}))?;
    shot(&mut c, "04-converted.png")?;

    // The README images and the GIF.
    let mut frames = Vec::new();
    for step in &STEPS {
        let img = ImageReader::open(work.join(step.png)).map_err(|e| e.to_string())?.decode().map_err(|e| e.to_string())?;
        let jpg = a.out.join(step.png).with_extension("jpg");
        let small = img.resize(1400, 1400, FilterType::Lanczos3).to_rgb8();
        let mut file = std::fs::File::create(&jpg).map_err(|e| e.to_string())?;
        let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut file, 82);
        enc.encode_image(&small).map_err(|e| e.to_string())?;
        println!("wrote {}", jpg.display());
        let frame = img.resize(900, 900, FilterType::Lanczos3).to_rgba8();
        frames.push(Frame::from_parts(frame, 0, 0, Delay::from_saturating_duration(Duration::from_millis(u64::from(step.gif_ms)))));
    }
    let gif_path = a.out.join("flow.gif");
    let mut gif = std::fs::File::create(&gif_path).map_err(|e| e.to_string())?;
    let mut enc = GifEncoder::new_with_speed(&mut gif, 10);
    enc.set_repeat(Repeat::Infinite).map_err(|e| e.to_string())?;
    enc.encode_frames(frames).map_err(|e| e.to_string())?;
    drop(enc);
    gif.flush().map_err(|e| e.to_string())?;
    let bytes = std::fs::metadata(&gif_path).map(|m| m.len()).unwrap_or(0);
    println!("wrote {} ({} KB, {} frames)", gif_path.display(), bytes / 1024, STEPS.len());
    Ok(())
}

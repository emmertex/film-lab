//! Starts PhotoCraft with the plug-ins installed and the sample negative open, for trying Film
//! Lab by hand.
//!
//! ```sh
//! scripts/manual-test.sh                  # builds the plug-ins, fetches the sample, runs this
//! scripts/manual-test.sh --plain          # the bare module, with no measurement inside
//! cargo run -p film-lab-companion --example manual_test -- --negative my-scan.tif
//! ```
//!
//! Launches PhotoCraft on its control channel with a work folder as its automation roots, opens
//! the negative, measures it the way `film-lab app` does (from a copy the app saves, so the
//! measurement is of the pixels the plug-in gets, whatever the app did on open), installs both
//! plug-ins (Convert Negative and Convert Negative (Advanced)) from the repository's build with
//! that measurement written inside (`--plain` installs the bare modules, which fall back to the
//! preset's base, or to the base picked in the advanced dialog), and stays attached until the app
//! quits (Ctrl-C quits both). While it runs, the
//! companion reaches the same instance: `film-lab app --dialog --root WORK --token-file
//! WORK/token`.
//!
//! Options: `--photocraft BIN` (or `$PHOTOCRAFT`; default: `../photocraft/target/release/photocraft`
//! next to this repository, then the installed app), `--negative FILE` (default: the Ektar
//! frame from `scripts/fetch-samples.sh`), `--work DIR` (default `target/manual`), `--port N`
//! (default 7878), `--plugin FILE` (installs that one module instead of the repository's builds),
//! `--plain`, and the measurement's options `--border N` and `--wb auto|neutral|warm|cool`.

use std::path::{Path, PathBuf};

use film_lab_companion::app::{Control, DEFAULT_ADDR, Launched, base64, find_photocraft, launch, measure_open_document};
use film_lab_companion::{AnalyzeOptions, Plugin, WbMode, find_plugin};
use serde_json::json;

struct Args {
    photocraft: Option<PathBuf>,
    negative: PathBuf,
    work: PathBuf,
    port: u16,
    plugin: Option<PathBuf>,
    plain: bool,
    analyze: AnalyzeOptions,
}

fn main() {
    if let Err(e) = run() {
        eprintln!("manual-test: {e}");
        std::process::exit(1);
    }
}

fn parse() -> Result<Args, String> {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut a = Args {
        photocraft: None,
        negative: repo.join("corpus/ektar-frame22-negative.png"),
        work: repo.join("target/manual"),
        port: 7878,
        plugin: None,
        plain: false,
        analyze: AnalyzeOptions::default(),
    };
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while let Some(arg) = args.get(i) {
        let value = |i: &mut usize| -> Result<&String, String> {
            *i += 1;
            args.get(*i).ok_or_else(|| format!("{arg} expects a value"))
        };
        match arg.as_str() {
            "--photocraft" => a.photocraft = Some(PathBuf::from(value(&mut i)?)),
            "--negative" => a.negative = PathBuf::from(value(&mut i)?),
            "--work" => a.work = PathBuf::from(value(&mut i)?),
            "--port" => a.port = value(&mut i)?.parse().map_err(|_| "--port expects a number")?,
            "--plugin" => a.plugin = Some(PathBuf::from(value(&mut i)?)),
            "--plain" => a.plain = true,
            "--border" => a.analyze.border_percent = value(&mut i)?.parse().map_err(|_| "--border expects a number")?,
            "--wb" => a.analyze.wb = WbMode::parse(value(&mut i)?).ok_or("--wb expects auto, neutral, warm or cool")?,
            other => return Err(format!("unknown option {other}")),
        }
        i += 1;
    }
    Ok(a)
}

fn run() -> Result<(), String> {
    let a = parse()?;
    if !a.negative.is_file() {
        return Err(format!("{} is missing: run scripts/fetch-samples.sh, or pass --negative FILE", a.negative.display()));
    }
    let photocraft = find_photocraft(a.photocraft.as_deref())?;
    let paths = match &a.plugin {
        Some(p) => vec![p.clone()],
        None => Plugin::ALL.into_iter().map(|p| find_plugin(None, p)).collect::<Result<_, _>>()?,
    };
    let mut modules = Vec::new();
    for path in &paths {
        modules.push(std::fs::read(path).map_err(|e| format!("cannot read the plug-in {}: {e}", path.display()))?);
    }

    // The negative goes into the work folder, the only place the app may open files from.
    std::fs::create_dir_all(&a.work).map_err(|e| format!("cannot create {}: {e}", a.work.display()))?;
    let work = a.work.canonicalize().map_err(|e| e.to_string())?;
    let name = a.negative.file_name().and_then(|n| n.to_str()).ok_or("the negative needs a file name")?.to_string();
    std::fs::copy(&a.negative, work.join(&name)).map_err(|e| format!("cannot copy the negative into {}: {e}", work.display()))?;

    println!("starting {} on port {}, automation root {}", photocraft.display(), a.port, work.display());
    let mut app = launch(&photocraft, &work, a.port)?;
    if let Err(e) = prepare(&app, &work, &name, &modules, (!a.plain).then_some(&a.analyze)) {
        app.child.kill().ok();
        app.child.wait().ok();
        return Err(e);
    }

    println!("PhotoCraft is running with Film Lab installed and {name} open.");
    if a.plain {
        println!("  Filter › Plug-ins › Film Lab: Convert Negative… (or ⌘F, then its name) opens the dialog; with no");
        println!("  measurement in the module it uses the preset's base. In Convert Negative (Advanced)…, base_r/g/b");
        println!("  take the border's colour as the Info panel shows it. The companion measures and installs afresh:");
    } else {
        println!("  Filter › Plug-ins › Film Lab: Convert Negative… (or ⌘F, then its name) converts with the measurement");
        println!("  of {name}; Convert Negative (Advanced)… also has it, and base_r/g/b override its film base.");
        println!("  The companion reaches this instance too:");
    }
    let connect = if app.addr == DEFAULT_ADDR { String::new() } else { format!(" --connect {}", a.port) };
    println!("  film-lab app --dialog --root {} --token-file {}{connect}", work.display(), work.join("token").display());
    println!("Quit PhotoCraft (or Ctrl-C here) to finish; its log is {}.", work.join("photocraft.log").display());
    let status = app.child.wait().map_err(|e| format!("waiting for PhotoCraft: {e}"))?;
    println!("PhotoCraft exited ({status}).");
    Ok(())
}

/// Installs `modules`, opens `name` in the launched app, and with `analyze` (none: the bare
/// modules) measures it there and installs them again with the measurement inside.
fn prepare(app: &Launched, work: &Path, name: &str, modules: &[Vec<u8>], analyze: Option<&AnalyzeOptions>) -> Result<(), String> {
    let mut c = Control::connect(&app.addr, &app.token)?;
    // Installed before the document opens: PhotoCraft brings its macOS menu bar up to date when
    // a document opens, not when a plug-in is installed over the channel, so a module installed
    // only afterwards is missing from Filter › Plug-ins until the next click in the window.
    // Replacing it later under the same id leaves the menu as it is.
    for module in modules {
        c.execute("plugin.install", json!({"data": base64(module), "replace": true}))?;
    }
    c.call("app.open", json!({"path": name}))?;
    if let Some(o) = analyze {
        let levels = measure_open_document(&mut c, work, o)?;
        println!("measurement from {} pixels: {}", levels.samples, serde_json::Value::Object(levels.measurement()));
        for module in modules {
            c.execute("plugin.install", json!({"data": base64(&levels.embed(module)?), "replace": true}))?;
        }
    }
    c.call("ui.set", json!({"fit": true})).ok();
    Ok(())
}

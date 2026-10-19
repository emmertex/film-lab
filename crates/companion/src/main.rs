//! `film-lab`: whole-image analysis and batch conversion for the PhotoCraft plug-in.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use std::path::{Path, PathBuf};

use film_lab_companion::{AnalyzeOptions, Plugin, WbMode, analyze, convert_one, convert_roll, find_photocraft_cli, find_plugin};
use serde_json::{Map, Value};

const USAGE: &str = "\
film-lab app [--dialog] [--advanced] [app options] [analysis options] [--set KEY=VALUE…]
film-lab analyze <scan> [analysis options]
film-lab convert <scan | folder> -o <file | folder> [analysis options] [conversion options]
film-lab compare <ours> <reference>
film-lab sample <scan> <x> <y> [--size N]

Analyses a scanned negative as a whole (film base, black and white points) and applies the
plug-in with the result: the automatic mode.

app converts the document open in a running PhotoCraft, nothing to type: the app saves a copy
for the analysis, then the plug-in runs with it (one history step), or with --dialog its dialog
opens with every value filled in, to adjust with the live preview before OK. Start PhotoCraft as
  photocraft --control 7878 --control-token-file TOKEN --automation-read-root DIR --automation-write-root DIR

App options:
  --connect ADDR      host:port of the control channel (default 127.0.0.1:7878)
  --token-file FILE   the file given to --control-token-file (or $PHOTOCRAFT_CONTROL_TOKEN_FILE)
  --token HEX         the token itself (or $PHOTOCRAFT_CONTROL_TOKEN)
  --root DIR          the app's automation write root (default: the current directory)
  --dialog            open the plug-in's dialog filled in instead of converting outright

Analysis options:
  --border N          percent of each edge left out of the analysis (default 8)
  --black-clip N      percent of pixels allowed below black (default 0.3)
  --white-clip N      percent of pixels allowed above white (default 0.3)
  --wb MODE           neutral (default: black and white neutral per channel, the medians
                      balanced too), auto (black and white only, the scene's colour kept),
                      warm, cool
  --master FILE       convert: analyse this scan and apply its analysis to the whole folder

Conversion options:
  --preset NAME       the film preset, spelled any way: Standard (default), Portra 160,
                      Portra 400, Ektar 100, Gold 200, ColorPlus 200, UltraMax 400,
                      Superia 400, C200, Pro 400H, CineStill 800, Monochrome
  --set KEY=VALUE     a plug-in parameter (exposure=0.5, brightness=-10, temp=8, …); repeatable
  --advanced          Film Lab: Convert Negative (Advanced) instead of Convert Negative: the film
                      base picked from the scan (--set filmBaseRed=… --set filmBaseGreen=…
                      --set filmBaseBlue=…, the border's colour as the Info panel shows it), tone,
                      colour and sharpening controls; a menu takes its option as the dialog
                      shows it (--set 'toneCurve=Cinematic Rich')
  --format EXT        output format for a folder (png, jpg, tif, …; default: as the input)
  --photocraft-cli P  the photocraft-cli binary (default: $PHOTOCRAFT_CLI, then the PATH)
  --plugin P          the plug-in .wasm (default: $FILM_LAB_WASM, then the repository's build
                      of the plug-in chosen)
  -h, --help          this text

compare prints per-channel statistics of both images, a 4 × 4 grid of mean colours and their
differences (both resampled to one size), to judge a conversion against a reference positive.
sample prints the average colour of the N × N square (default 11) at a pixel, on the 0..255 scale
the plug-in's base_* and neutral_* parameters take, as the app's Info panel shows it.
";

fn main() {
    if let Err(e) = run(std::env::args().skip(1).collect()) {
        eprintln!("film-lab: {e}");
        std::process::exit(1);
    }
}

struct Args {
    positional: Vec<String>,
    out: Option<PathBuf>,
    options: AnalyzeOptions,
    master: Option<PathBuf>,
    set: Map<String, Value>,
    format: Option<String>,
    cli: Option<PathBuf>,
    plugin: Option<PathBuf>,
    which: Plugin,
    size: u32,
    connect: String,
    token: Option<String>,
    token_file: Option<PathBuf>,
    root: PathBuf,
    dialog: bool,
}

fn parse(args: &[String]) -> Result<Option<Args>, String> {
    let mut a = Args {
        positional: Vec::new(),
        out: None,
        options: AnalyzeOptions::default(),
        master: None,
        set: Map::new(),
        format: None,
        cli: None,
        plugin: None,
        which: Plugin::Convert,
        size: 11,
        connect: film_lab_companion::app::DEFAULT_ADDR.to_string(),
        token: None,
        token_file: None,
        root: PathBuf::from("."),
        dialog: false,
    };
    let mut i = 0;
    while let Some(arg) = args.get(i) {
        let value = |i: &mut usize| -> Result<&String, String> {
            *i += 1;
            args.get(*i).ok_or_else(|| format!("{arg} expects a value"))
        };
        let number = |i: &mut usize| -> Result<f32, String> { value(i)?.parse().map_err(|_| format!("{arg} expects a number")) };
        match arg.as_str() {
            "-h" | "--help" => return Ok(None),
            "-o" | "--out" => a.out = Some(PathBuf::from(value(&mut i)?)),
            "--border" => a.options.border_percent = number(&mut i)?,
            "--black-clip" => a.options.black_clip = number(&mut i)?,
            "--white-clip" => a.options.white_clip = number(&mut i)?,
            "--wb" => a.options.wb = WbMode::parse(value(&mut i)?).ok_or("--wb expects auto, neutral, warm or cool")?,
            "--master" => a.master = Some(PathBuf::from(value(&mut i)?)),
            "--set" => {
                let kv = value(&mut i)?;
                let (k, v) = kv.split_once('=').ok_or_else(|| format!("--set expects KEY=VALUE, got {kv}"))?;
                let v = v.parse::<f64>().map_or_else(|_| Value::String(v.to_string()), Value::from);
                a.set.insert(k.to_string(), v);
            }
            "--preset" => {
                let name = value(&mut i)?;
                let preset = film_lab_core::Preset::parse(name).ok_or_else(|| {
                    format!("unknown preset `{name}`; one of {}", film_lab_core::PRESETS.iter().map(|p| p.name).collect::<Vec<_>>().join(", "))
                })?;
                // The host checks the value against the manifest's spelling.
                a.set.insert("preset".into(), Value::String(preset.name.to_string()));
            }
            "--format" => a.format = Some(value(&mut i)?.clone()),
            "--photocraft-cli" => a.cli = Some(PathBuf::from(value(&mut i)?)),
            "--plugin" => a.plugin = Some(PathBuf::from(value(&mut i)?)),
            "--size" => a.size = value(&mut i)?.parse().map_err(|_| "--size expects a whole number")?,
            "--connect" => {
                let v = value(&mut i)?;
                a.connect = if v.contains(':') { v.clone() } else { format!("127.0.0.1:{v}") };
            }
            "--token" => a.token = Some(value(&mut i)?.clone()),
            "--token-file" => a.token_file = Some(PathBuf::from(value(&mut i)?)),
            "--root" => a.root = PathBuf::from(value(&mut i)?),
            "--dialog" => a.dialog = true,
            "--advanced" => a.which = Plugin::Advanced,
            other if other.starts_with('-') => return Err(format!("unknown option `{other}`\n\n{USAGE}")),
            _ => a.positional.push(arg.clone()),
        }
        i += 1;
    }
    Ok(Some(a))
}

fn run(args: Vec<String>) -> Result<(), String> {
    let Some(a) = parse(&args)? else {
        print!("{USAGE}");
        return Ok(());
    };
    if a.positional.first().map(String::as_str) == Some("app") {
        let o = film_lab_companion::app::AppOptions {
            addr: a.connect.clone(),
            token: film_lab_companion::app::find_token(a.token.as_deref(), a.token_file.as_deref())?,
            root: a.root.clone(),
            dialog: a.dialog,
            analyze: a.options.clone(),
            set: a.set.clone(),
            plugin: a.which,
            wasm: find_plugin(a.plugin.as_deref(), a.which)?,
        };
        let r = film_lab_companion::app::convert_in_app(&o)?;
        println!("{}", serde_json::to_string_pretty(&r).map_err(|e| e.to_string())?);
        return Ok(());
    }
    let (Some(command), Some(target)) = (a.positional.first(), a.positional.get(1)) else {
        return Err(format!("missing command or scan\n\n{USAGE}"));
    };
    let target = Path::new(target);
    match command.as_str() {
        "analyze" => {
            let levels = analyze(&open(target)?, &a.options)?;
            let mut params = levels.plugin_params();
            params.extend(a.set.clone());
            let out = serde_json::json!({"measurement": levels.measurement(), "params": params, "samples": levels.samples});
            println!("{}", serde_json::to_string_pretty(&out).map_err(|e| e.to_string())?);
            Ok(())
        }
        "convert" => {
            let out = a.out.as_deref().ok_or("convert needs -o <file | folder>")?;
            let cli = find_photocraft_cli(a.cli.as_deref());
            let wasm = find_plugin(a.plugin.as_deref(), a.which)?;
            let master = a.master.as_deref().or((!target.is_dir()).then_some(target)).map(open).transpose()?;
            if target.is_dir() {
                let analysed = match master {
                    Some(img) => Some(analyze(&img, &a.options)?),
                    None => None,
                };
                let Some(levels) = analysed else {
                    return Err("a folder needs --master FILE: the scan whose analysis applies to the whole roll".into());
                };
                let mut params = levels.plugin_params();
                params.extend(a.set.clone());
                eprintln!("roll measurement from {} pixels: {}", levels.samples, Value::Object(levels.measurement()));
                convert_roll(&cli, a.which, &wasm, &levels, target, out, &params, a.format.as_deref())
            } else {
                let levels = analyze(master.as_ref().ok_or("no scan")?, &a.options)?;
                let mut params = levels.plugin_params();
                params.extend(a.set.clone());
                eprintln!("measurement from {} pixels: {}", levels.samples, Value::Object(levels.measurement()));
                convert_one(&cli, a.which, &wasm, &levels, target, out, &params)
            }
        }
        "sample" => {
            let (Some(x), Some(y)) = (a.positional.get(2), a.positional.get(3)) else { return Err("sample needs <scan> <x> <y>".into()) };
            let (x, y) = (x.parse::<u32>().map_err(|_| "x must be a whole number")?, y.parse::<u32>().map_err(|_| "y must be a whole number")?);
            let c = film_lab_companion::sample(&open(target)?, x, y, a.size);
            println!(
                "{}",
                serde_json::json!({"x": x, "y": y, "size": a.size, "r": (c[0] * 10.0).round() / 10.0, "g": (c[1] * 10.0).round() / 10.0, "b": (c[2] * 10.0).round() / 10.0})
            );
            Ok(())
        }
        "compare" => {
            let reference = a.positional.get(2).ok_or("compare needs <ours> <reference>")?;
            let report = film_lab_companion::compare::report(&open(target)?, &open(Path::new(reference))?);
            println!("{}", serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?);
            Ok(())
        }
        other => Err(format!("unknown command `{other}`\n\n{USAGE}")),
    }
}

fn open(path: &Path) -> Result<image::DynamicImage, String> {
    image::open(path).map_err(|e| format!("cannot read {}: {e}", path.display()))
}

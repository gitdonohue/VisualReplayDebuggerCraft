mod render;

use std::process::ExitCode;

use vrd_format::Compression;
use vrd_model::Replay;

const USAGE: &str = "\
vrd-cli: inspect and convert VisualReplayDebugger captures

USAGE:
    vrd-cli info <file>                 Summarize a capture
    vrd-cli logs <file> [--limit N]     Print log entries
    vrd-cli entities <file> [--frame N] List entities with their position at a frame
    vrd-cli convert <in> <out> [--raw]  Rewrite a capture (deflate-compressed unless --raw)
";
const RENDER_HELP: &str = render::USAGE;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("info") => info(args.get(1).ok_or(USAGE)?),
        Some("logs") => {
            let path = args.get(1).ok_or(USAGE)?;
            let limit = match args.iter().position(|a| a == "--limit") {
                Some(i) => args.get(i + 1).and_then(|v| v.parse().ok()).ok_or("--limit needs a number")?,
                None => usize::MAX,
            };
            logs(path, limit)
        }
        Some("entities") => {
            let path = args.get(1).ok_or(USAGE)?;
            let frame = match args.iter().position(|a| a == "--frame") {
                Some(i) => args.get(i + 1).and_then(|v| v.parse().ok()).ok_or("--frame needs a number")?,
                None => 0,
            };
            entities(path, frame)
        }
        Some("render") => render::run(&args[1..]),
        Some("convert") => {
            let (input, output) = (args.get(1).ok_or(USAGE)?, args.get(2).ok_or(USAGE)?);
            convert(input, output, args.iter().any(|a| a == "--raw"))
        }
        Some("-h" | "--help" | "help") | None => {
            println!("{USAGE}{RENDER_HELP}");
            Ok(())
        }
        Some(other) => Err(format!("unknown command `{other}`\n\n{USAGE}")),
    }
}

fn load(path: &str) -> Result<Replay, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("cannot read {path}: {e}"))?;
    Replay::from_bytes(&bytes).map_err(|e| format!("{path}: {e}"))
}

fn info(path: &str) -> Result<(), String> {
    let r = load(path)?;
    let i = r.info();
    println!("file:        {path}");
    println!(
        "format:      {}{}",
        if i.compressed { "deflate-compressed" } else { "uncompressed" },
        if i.truncated { " (truncated)" } else { "" }
    );
    if let Some(w) = &i.warning {
        println!("warning:     {w}");
    }
    println!("blocks:      {}", i.record_count);
    println!("frames:      {} ({:.3} s)", r.frame_count(), r.total_time());
    println!("entities:    {}", r.entity_count());
    println!("logs:        {}", r.logs().len());
    println!("draws:       {}", r.draws().len());
    println!("categories:  {}", join(r.entity_categories().iter()));
    println!("log cats:    {}", join(r.log_categories().iter()));
    println!("draw cats:   {}", join(r.draw_categories().iter()));
    println!("parameters:  {} names", r.param_names().len());
    positions(&r);
    Ok(())
}

fn join<'a>(items: impl Iterator<Item = &'a String>) -> String {
    let all: Vec<_> = items.map(|s| if s.is_empty() { "(none)" } else { s.as_str() }).collect();
    if all.is_empty() { "-".to_string() } else { all.join(", ") }
}

fn logs(path: &str, limit: usize) -> Result<(), String> {
    let r = load(path)?;
    for l in r.logs().iter().take(limit) {
        let who = r.entity(l.entity).map_or_else(|| format!("#{}", l.entity), |e| e.display_name());
        println!("[{:>6} {:>9.3}s] {:<16} {:<20} {}", l.frame, r.time_for_frame(l.frame), l.category, who, l.message);
    }
    Ok(())
}

fn convert(input: &str, output: &str, raw: bool) -> Result<(), String> {
    let bytes = std::fs::read(input).map_err(|e| format!("cannot read {input}: {e}"))?;
    let decoded = vrd_format::decode(&bytes).map_err(|e| format!("{input}: {e}"))?;
    let compression = if raw { Compression::None } else { Compression::Deflate };
    let out = vrd_format::encode(&decoded.records, compression).map_err(|e| e.to_string())?;
    std::fs::write(output, &out).map_err(|e| format!("cannot write {output}: {e}"))?;
    println!("wrote {} blocks to {output} ({} bytes)", decoded.records.len(), out.len());
    Ok(())
}

fn positions(r: &Replay) {
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    let mut moving = 0;
    let mut samples = 0;
    for e in r.entities().filter(|e| !e.transforms.is_empty()) {
        moving += 1;
        for (_, t) in e.transforms.iter() {
            samples += 1;
            let p = [t.translation.x, t.translation.y, t.translation.z];
            for i in 0..3 {
                lo[i] = lo[i].min(p[i]);
                hi[i] = hi[i].max(p[i]);
            }
        }
    }
    if samples == 0 {
        println!("positions:   none");
        return;
    }
    println!("positions:   {moving} entities, {samples} samples");
    for (axis, i) in [("x", 0), ("y", 1), ("z", 2)] {
        println!("             {axis}: {:.1} .. {:.1}", lo[i], hi[i]);
    }
    let named: Vec<_> = r.entities().filter(|e| e.name == "maincamera").collect();
    for e in named {
        println!("maincamera:  id {} {} transforms", e.id, e.transforms.len());
    }
    let (mut mesh, mut sphere, mut capsule, mut boxes, mut line, mut circle) = (0, 0, 0, 0, 0, 0);
    for d in r.draws().iter().filter(|d| d.is_creation) {
        match d.shape {
            vrd_model::Shape::Mesh { .. } => mesh += 1,
            vrd_model::Shape::Sphere { .. } => sphere += 1,
            vrd_model::Shape::Capsule { .. } => capsule += 1,
            vrd_model::Shape::Box { .. } => boxes += 1,
            vrd_model::Shape::Line { .. } => line += 1,
            vrd_model::Shape::Circle { .. } => circle += 1,
        }
    }
    println!(
        "creation:    {mesh} meshes, {sphere} spheres, {capsule} capsules, {boxes} boxes, {line} lines, {circle} circles"
    );
    let mut per_frame = std::collections::BTreeMap::<&str, usize>::new();
    for d in r.draws().iter().filter(|d| !d.is_creation) {
        let kind = match d.shape {
            vrd_model::Shape::Mesh { .. } => "mesh",
            vrd_model::Shape::Sphere { .. } => "sphere",
            vrd_model::Shape::Capsule { .. } => "capsule",
            vrd_model::Shape::Box { .. } => "box",
            vrd_model::Shape::Line { .. } => "line",
            vrd_model::Shape::Circle { .. } => "circle",
        };
        *per_frame.entry(kind).or_default() += 1;
    }
    println!("per-frame:   {per_frame:?}");
}
fn entities(path: &str, frame: i32) -> Result<(), String> {
    let r = load(path)?;
    let frame = frame.clamp(0, r.last_frame());
    println!("{:>4} {:>6} {:<34} {:>28}  geometry", "id", "parent", "name", format!("position @ frame {frame}"));
    for e in r.entities() {
        let pos = match r.position_at(e.id, frame) {
            Some(p) if !e.transforms.is_empty() => format!("({:.2}, {:.2}, {:.2})", p.x, p.y, p.z),
            _ => "-".to_string(),
        };
        let alive = if r.is_alive_at(e.id, frame) { "" } else { " (inactive)" };
        let draws = r.creation_draws(e.id).count();
        println!(
            "{:>4} {:>6} {:<34} {:>28}  {} creation draws{}",
            e.id,
            e.parent.map_or("-".to_string(), |p| p.to_string()),
            e.display_name().chars().take(34).collect::<String>(),
            pos,
            draws,
            alive
        );
    }
    Ok(())
}

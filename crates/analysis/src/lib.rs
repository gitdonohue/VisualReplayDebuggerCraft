//! The tools the MCP server offers. Each takes the `path` of a capture.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value, json};
use vrd_model::{Entity, FrameList, LogEntry, Point, Replay};

/// A tool call that went wrong in a way the assistant can read and correct.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolError(pub String);

impl From<String> for ToolError {
    fn from(message: String) -> Self {
        ToolError(message)
    }
}

impl From<&str> for ToolError {
    fn from(message: &str) -> Self {
        ToolError(message.to_string())
    }
}

const MAX_LIMIT: usize = 1000;

type Outcome = Result<Value, ToolError>;

fn tool(name: &str, description: &str, properties: &Value, required: &[&str]) -> Value {
    let mut props = json!({ "path": { "type": "string", "description": "Path to the .vrd capture" } });
    if let (Some(base), Some(extra)) = (props.as_object_mut(), properties.as_object()) {
        base.extend(extra.clone());
    }
    let mut needed = vec!["path"];
    needed.extend_from_slice(required);
    json!({
        "name": name,
        "description": description,
        "inputSchema": { "type": "object", "properties": props, "required": needed },
    })
}

fn merge(a: &Value, b: &Value) -> Value {
    let mut m = a.as_object().cloned().unwrap_or_default();
    m.extend(b.as_object().cloned().unwrap_or_default());
    Value::Object(m)
}

/// The tools and their JSON schemas. with_path: each takes the path of a capture to read (the MCP
/// server); without it the tools act on whichever capture the host has open.
pub fn definitions(with_path: bool) -> Vec<Value> {
    let mut all = tool_definitions();
    if !with_path {
        for tool in &mut all {
            if let Some(properties) = tool["inputSchema"]["properties"].as_object_mut() {
                properties.remove("path");
            }
            if let Some(required) = tool["inputSchema"]["required"].as_array_mut() {
                required.retain(|r| r != "path");
            }
        }
    }
    all
}

fn tool_definitions() -> Vec<Value> {
    let entity =
        json!({ "entity": { "description": "Entity id (number) or name/path (text; a unique part of it is enough)" } });
    let range = json!({
        "from_frame": { "type": "integer", "description": "First frame to include" },
        "to_frame": { "type": "integer", "description": "Last frame to include" },
    });
    vec![
        tool(
            "capture_info",
            "Summarise a capture: frames, duration, entity and log counts, categories and parameter names.",
            &json!({}),
            &[],
        ),
        tool(
            "list_entities",
            "List entities with their lifetime (frames), position at a frame and how much data they have. \
             Filter by name/path text or category. Paged.",
            &json!({
                "filter": { "type": "string", "description": "Keep entities whose name or path contains this (case-insensitive)" },
                "category": { "type": "string", "description": "Keep entities of this category" },
                "frame": { "type": "integer", "description": "Frame for positions and `alive` (default 0)" },
                "alive_only": { "type": "boolean", "description": "Only entities that exist at `frame`" },
                "with_logs": { "type": "boolean", "description": "Only entities that logged something" },
                "limit": { "type": "integer", "description": "Default 100, at most 1000" },
                "offset": { "type": "integer" },
            }),
            &[],
        ),
        tool(
            "get_entity",
            "Everything about one entity at a frame: position, numeric and text parameters, static parameters, \
             children, lifetime and what data series it has.",
            &merge(&entity, &json!({ "frame": { "type": "integer", "description": "Default 0" } })),
            &["entity"],
        ),
        tool(
            "search_logs",
            "Search the log entries by text in the message, category, entity and frame range. Paged.",
            &merge(
                &range,
                &json!({
                    "text": { "type": "string", "description": "Case-insensitive text in the message" },
                    "category": { "type": "string" },
                    "entity": entity["entity"].clone(),
                    "limit": { "type": "integer", "description": "Default 50, at most 1000" },
                    "offset": { "type": "integer" },
                }),
            ),
            &[],
        ),
        tool(
            "log_summary",
            "Where the log entries are: counts by category and by entity, and the busiest frames.",
            &json!({ "top": { "type": "integer", "description": "How many entities and frames to list (default 10)" } }),
            &[],
        ),
        tool(
            "get_series",
            "Numeric parameters of an entity over time. Without `name`: every series with its statistics. With \
             `name`: the values as [frame, time, value] reduced to `max_points` (keeping the extremes), with statistics.",
            &merge(
                &merge(&entity, &range),
                &json!({
                    "name": { "type": "string", "description": "Series name, such as FPS" },
                    "max_points": { "type": "integer", "description": "Default 200" },
                }),
            ),
            &["entity"],
        ),
        tool(
            "find_spikes",
            "Find the values of a numeric series that are far from its mean (more than `sigma` standard deviations).",
            &merge(
                &entity,
                &json!({
                    "name": { "type": "string", "description": "Series name" },
                    "sigma": { "type": "number", "description": "Default 3" },
                    "limit": { "type": "integer", "description": "Default 20" },
                }),
            ),
            &["entity", "name"],
        ),
        tool(
            "get_param_timeline",
            "Text parameters of an entity over time (states, modes). Without `name`: each parameter with how often \
             it changes. With `name`: its runs (value, frames, times).",
            &merge(
                &merge(&entity, &range),
                &json!({
                    "name": { "type": "string", "description": "Parameter name, such as MoveState" },
                    "limit": { "type": "integer", "description": "Most runs to return (default 200)" },
                    "offset": { "type": "integer" },
                }),
            ),
            &["entity"],
        ),
        tool(
            "get_trajectory",
            "Where an entity moved: sampled positions as [frame, x, y, z], distance travelled, bounds and average speed.",
            &merge(
                &merge(&entity, &range),
                &json!({ "max_points": { "type": "integer", "description": "Default 100" } }),
            ),
            &["entity"],
        ),
        tool(
            "get_frame",
            "A snapshot of one frame: its time, the entities that exist and where, the log entries written on it and \
             how many debug draws it has per category.",
            &json!({
                "frame": { "type": "integer" },
                "limit": { "type": "integer", "description": "Most entities and log entries to list (default 100)" },
            }),
            &["frame"],
        ),
    ]
}

/// Runs the tool `name` on `replay`.
pub fn call(replay: &Replay, name: &str, args: &Value) -> Outcome {
    if !tool_definitions().iter().any(|d| d["name"] == name) {
        return Err(format!("unknown tool `{name}`").into());
    }
    match name {
        "capture_info" => Ok(capture_info(replay)),
        "list_entities" => Ok(list_entities(replay, args)),
        "get_entity" => get_entity(replay, args),
        "search_logs" => search_logs(replay, args),
        "log_summary" => Ok(log_summary(replay, args)),
        "get_series" => get_series(replay, args),
        "find_spikes" => find_spikes(replay, args),
        "get_param_timeline" => get_param_timeline(replay, args),
        "get_trajectory" => get_trajectory(replay, args),
        _ => get_frame(replay, args),
    }
}

/// The id of the entity a JSON value names: an id, or text matching a name or path.
pub fn resolve_entity(r: &Replay, value: &Value) -> Result<i32, ToolError> {
    entity_arg(r, &json!({ "entity": value })).map(|e| e.id)
}

// Arguments and values

fn str_arg<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(Value::as_str).filter(|s| !s.is_empty())
}

fn int_arg(args: &Value, key: &str) -> Option<i32> {
    args.get(key).and_then(Value::as_i64).map(|v| v.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32)
}

fn count_arg(args: &Value, key: &str, default: usize) -> usize {
    args.get(key).and_then(Value::as_u64).map_or(default, |v| v as usize).clamp(1, MAX_LIMIT)
}

fn bool_arg(args: &Value, key: &str) -> bool {
    args.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn num(v: f32) -> Value {
    json!((f64::from(v) * 10_000.0).round() / 10_000.0)
}

fn secs(v: f64) -> Value {
    json!((v * 10_000.0).round() / 10_000.0)
}

fn point(p: Point) -> Value {
    json!([num(p.x), num(p.y), num(p.z)])
}

fn frame_range(r: &Replay, args: &Value) -> (i32, i32) {
    let last = r.last_frame();
    let from = int_arg(args, "from_frame").unwrap_or(0).clamp(0, last);
    let to = int_arg(args, "to_frame").unwrap_or(last).clamp(from, last);
    (from, to)
}

/// The entity an argument names: an id, or text matching a name or path.
fn entity_arg<'a>(r: &'a Replay, args: &Value) -> Result<&'a Entity, ToolError> {
    let value = args.get("entity").ok_or("the `entity` argument is required")?;
    if let Some(id) = value.as_i64() {
        return r.entity(id as i32).ok_or_else(|| format!("no entity with id {id}").into());
    }
    let text = value.as_str().ok_or("`entity` is an id (number) or a name (text)")?.trim();
    if let Ok(id) = text.parse::<i32>()
        && let Some(e) = r.entity(id)
    {
        return Ok(e);
    }
    let lower = text.to_lowercase();
    let stages: [&dyn Fn(&Entity) -> bool; 4] = [
        &|e| e.display_name() == text || e.path == text,
        &|e| e.display_name().to_lowercase() == lower || e.path.to_lowercase() == lower,
        &|e| e.display_name().to_lowercase().contains(&lower),
        &|e| e.path.to_lowercase().contains(&lower),
    ];
    for matches in stages {
        let found: Vec<&Entity> = r.entities().filter(|e| matches(e)).collect();
        match found.as_slice() {
            [] => {}
            [one] => return Ok(one),
            many => {
                let names: Vec<String> =
                    many.iter().take(8).map(|e| format!("{} (id {})", e.display_name(), e.id)).collect();
                return Err(format!(
                    "`{text}` matches {} entities: {}{}. Use an id or a longer name.",
                    many.len(),
                    names.join(", "),
                    if many.len() > 8 { ", ..." } else { "" }
                )
                .into());
            }
        }
    }
    Err(format!("no entity matches `{text}`. Use list_entities to see what exists.").into())
}

fn page<T: Clone>(items: &[T], args: &Value, default: usize) -> (Vec<T>, usize) {
    let limit = count_arg(args, "limit", default);
    let offset = args.get("offset").and_then(Value::as_u64).unwrap_or(0) as usize;
    (items.iter().skip(offset).take(limit).cloned().collect(), offset)
}

// Statistics

struct Stats {
    count: usize,
    min: f32,
    max: f32,
    mean: f64,
    std: f64,
}

fn stats(values: &[f32]) -> Option<Stats> {
    if values.is_empty() {
        return None;
    }
    let count = values.len();
    let (min, max) = values.iter().fold((f32::MAX, f32::MIN), |(lo, hi), v| (lo.min(*v), hi.max(*v)));
    let mean = values.iter().map(|v| f64::from(*v)).sum::<f64>() / count as f64;
    let variance = values.iter().map(|v| (f64::from(*v) - mean).powi(2)).sum::<f64>() / count as f64;
    Some(Stats { count, min, max, mean, std: variance.sqrt() })
}

fn stats_json(s: &Stats) -> Value {
    json!({
        "samples": s.count,
        "min": num(s.min),
        "max": num(s.max),
        "mean": secs(s.mean),
        "std_dev": secs(s.std),
    })
}

fn values_of(points: &[(i32, f32)]) -> Vec<f32> {
    points.iter().map(|p| p.1).collect()
}

/// At most about `max` points, keeping the lowest and highest of each stretch so spikes survive.
fn decimate(points: &[(i32, f32)], max: usize) -> Vec<(i32, f32)> {
    if points.len() <= max {
        return points.to_vec();
    }
    let buckets = (max / 2).max(1);
    let mut out = Vec::with_capacity(buckets * 2);
    for b in 0..buckets {
        let chunk = &points[b * points.len() / buckets..(b + 1) * points.len() / buckets];
        let (Some(low), Some(high)) = (
            chunk.iter().copied().min_by(|a, b| a.1.total_cmp(&b.1)),
            chunk.iter().copied().max_by(|a, b| a.1.total_cmp(&b.1)),
        ) else {
            continue;
        };
        if low.0 <= high.0 {
            out.push(low);
            if high != low {
                out.push(high);
            }
        } else {
            out.push(high);
            out.push(low);
        }
    }
    out
}

fn series_points(list: &FrameList<f32>, from: i32, to: i32) -> Vec<(i32, f32)> {
    list.range(from, to).map(|(f, v)| (f, *v)).filter(|(_, v)| v.is_finite()).collect()
}

// Tools

fn capture_info(r: &Replay) -> Value {
    let info = r.info();
    json!({
        "compressed": info.compressed,
        "truncated": info.truncated,
        "warning": info.warning,
        "frames": r.frame_count(),
        "last_frame": r.last_frame(),
        "duration_seconds": secs(r.total_time()),
        "entities": r.entity_count(),
        "logs": r.logs().len(),
        "debug_draws": r.draws().len(),
        "entity_categories": r.entity_categories(),
        "log_categories": r.log_categories(),
        "draw_categories": r.draw_categories(),
        "parameter_names": r.param_names(),
    })
}

fn position_json(r: &Replay, e: &Entity, frame: i32) -> Value {
    if e.transforms.is_empty() { Value::Null } else { r.position_at(e.id, frame).map_or(Value::Null, point) }
}

fn entity_json(r: &Replay, e: &Entity, frame: i32) -> Value {
    let (first, last) = r.lifetime(e.id).unwrap_or((e.creation_frame, r.last_frame()));
    json!({
        "id": e.id,
        "name": e.display_name(),
        "path": e.path,
        "type": e.type_name,
        "category": e.category,
        "parent": e.parent,
        "first_frame": first,
        "last_frame": last,
        "alive": r.is_alive_at(e.id, frame),
        "position": position_json(r, e, frame),
        "log_entries": e.log_frames.len(),
        "numeric_series": e.values.len(),
        "text_parameters": e.params.len(),
    })
}

fn list_entities(r: &Replay, args: &Value) -> Value {
    let frame = int_arg(args, "frame").unwrap_or(0).clamp(0, r.last_frame());
    let filter = str_arg(args, "filter").map(str::to_lowercase);
    let category = str_arg(args, "category").map(str::to_lowercase);
    let matching: Vec<&Entity> = r
        .entities()
        .filter(|e| {
            filter
                .as_ref()
                .is_none_or(|f| e.display_name().to_lowercase().contains(f) || e.path.to_lowercase().contains(f))
                && category.as_ref().is_none_or(|c| e.category.to_lowercase() == *c)
                && (!bool_arg(args, "alive_only") || r.is_alive_at(e.id, frame))
                && (!bool_arg(args, "with_logs") || e.has_logs())
        })
        .collect();
    let (shown, offset) = page(&matching, args, 100);
    json!({
        "frame": frame,
        "total": matching.len(),
        "offset": offset,
        "entities": shown.iter().map(|e| entity_json(r, e, frame)).collect::<Vec<_>>(),
    })
}

fn get_entity(r: &Replay, args: &Value) -> Outcome {
    let e = entity_arg(r, args)?;
    let frame = int_arg(args, "frame").unwrap_or(0).clamp(0, r.last_frame());
    let to_map = |pairs: Vec<(String, Value)>| Value::Object(pairs.into_iter().collect::<Map<String, Value>>());
    let mut out = entity_json(r, e, frame);
    let fields = out.as_object_mut().expect("an object");
    fields.insert("frame".into(), json!(frame));
    fields.insert("time".into(), secs(r.time_for_frame(frame)));
    fields.insert(
        "text_parameters".into(),
        to_map(r.params_at(e.id, frame).into_iter().map(|(k, v)| (k.to_string(), json!(v))).collect()),
    );
    fields.insert(
        "numeric_parameters".into(),
        to_map(r.values_at(e.id, frame).into_iter().map(|(k, v)| (k.to_string(), num(v))).collect()),
    );
    fields.insert(
        "static_parameters".into(),
        to_map(e.static_params.iter().map(|(k, v)| (k.clone(), json!(v))).collect()),
    );
    fields.insert("children".into(), json!(r.children_of(e.id)));
    fields.insert("series_names".into(), json!(e.values.keys().collect::<Vec<_>>()));
    fields.insert("creation_draws".into(), json!(e.creation_draws.len()));
    fields.insert("transform_samples".into(), json!(e.transforms.len()));
    Ok(out)
}

fn search_logs(r: &Replay, args: &Value) -> Outcome {
    let (from, to) = frame_range(r, args);
    let text = str_arg(args, "text").map(str::to_lowercase);
    let category = str_arg(args, "category").map(str::to_lowercase);
    let entity = if args.get("entity").is_some() { Some(entity_arg(r, args)?.id) } else { None };
    let matching: Vec<&LogEntry> = r
        .logs_in(from, to)
        .iter()
        .filter(|l| {
            text.as_ref().is_none_or(|t| l.message.to_lowercase().contains(t))
                && category.as_ref().is_none_or(|c| l.category.to_lowercase() == *c)
                && entity.is_none_or(|id| l.entity == id)
        })
        .collect();
    let (shown, offset) = page(&matching, args, 50);
    Ok(json!({
        "total": matching.len(),
        "offset": offset,
        "entries": shown.iter().map(|l| json!({
            "frame": l.frame,
            "time": secs(r.time_for_frame(l.frame)),
            "category": l.category,
            "entity_id": l.entity,
            "entity": r.entity(l.entity).map(Entity::display_name),
            "message": l.message,
        })).collect::<Vec<_>>(),
    }))
}

fn ranked<K: Ord + Copy>(counts: &BTreeMap<K, usize>) -> Vec<(K, usize)> {
    let mut v: Vec<(K, usize)> = counts.iter().map(|(k, n)| (*k, *n)).collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    v
}

fn log_summary(r: &Replay, args: &Value) -> Value {
    let top = count_arg(args, "top", 10);
    let mut by_category = BTreeMap::<&str, usize>::new();
    let mut by_entity = BTreeMap::<i32, usize>::new();
    let mut by_frame = BTreeMap::<i32, usize>::new();
    for l in r.logs() {
        *by_category.entry(l.category.as_str()).or_default() += 1;
        *by_entity.entry(l.entity).or_default() += 1;
        *by_frame.entry(l.frame).or_default() += 1;
    }
    json!({
        "total": r.logs().len(),
        "first_frame": r.logs().first().map(|l| l.frame),
        "last_frame": r.logs().last().map(|l| l.frame),
        "by_category": ranked(&by_category).iter().map(|(k, n)| json!({ "category": k, "count": n })).collect::<Vec<_>>(),
        "top_entities": ranked(&by_entity).iter().take(top).map(|(id, n)| {
            json!({ "id": id, "name": r.entity(*id).map(Entity::display_name), "count": n })
        }).collect::<Vec<_>>(),
        "busiest_frames": ranked(&by_frame).iter().take(top).map(|(f, n)| {
            json!({ "frame": f, "time": secs(r.time_for_frame(*f)), "count": n })
        }).collect::<Vec<_>>(),
    })
}

fn series_of<'a>(e: &'a Entity, args: &Value) -> Result<(&'a str, &'a FrameList<f32>), ToolError> {
    let name = str_arg(args, "name").ok_or("the `name` argument is required")?;
    let lower = name.to_lowercase();
    let found = e.values.get_key_value(name).or_else(|| e.values.iter().find(|(k, _)| k.to_lowercase() == lower));
    found.map(|(k, v)| (k.as_str(), v)).ok_or_else(|| {
        format!(
            "{} has no numeric series `{name}`. It has: {}",
            e.display_name(),
            e.values.keys().cloned().collect::<Vec<_>>().join(", ")
        )
        .into()
    })
}

fn get_series(r: &Replay, args: &Value) -> Outcome {
    let e = entity_arg(r, args)?;
    let (from, to) = frame_range(r, args);
    if str_arg(args, "name").is_none() {
        let series: Vec<Value> = e
            .values
            .iter()
            .filter_map(|(name, list)| {
                let points = series_points(list, from, to);
                let s = stats(&values_of(&points))?;
                let mut v = stats_json(&s);
                let fields = v.as_object_mut().expect("an object");
                fields.insert("name".into(), json!(name));
                fields.insert("first".into(), num(points[0].1));
                fields.insert("last".into(), num(points[points.len() - 1].1));
                Some(v)
            })
            .collect();
        return Ok(json!({ "entity": e.display_name(), "from_frame": from, "to_frame": to, "series": series }));
    }
    let (name, list) = series_of(e, args)?;
    let points = series_points(list, from, to);
    let shown = decimate(&points, count_arg(args, "max_points", 200));
    Ok(json!({
        "entity": e.display_name(),
        "name": name,
        "from_frame": from,
        "to_frame": to,
        "stats": stats(&values_of(&points)).map_or(Value::Null, |s| stats_json(&s)),
        "total_samples": points.len(),
        "points": shown.iter().map(|(f, v)| json!([f, secs(r.time_for_frame(*f)), num(*v)])).collect::<Vec<_>>(),
    }))
}

fn find_spikes(r: &Replay, args: &Value) -> Outcome {
    let e = entity_arg(r, args)?;
    let (name, list) = series_of(e, args)?;
    let sigma = args.get("sigma").and_then(Value::as_f64).filter(|s| *s > 0.0).unwrap_or(3.0);
    let points = series_points(list, 0, r.last_frame());
    let s = stats(&values_of(&points)).ok_or("the series has no values")?;
    let mut spikes: Vec<(f64, i32, f32)> = points
        .iter()
        .map(|(f, v)| (((f64::from(*v) - s.mean) / s.std).abs(), *f, *v))
        .filter(|(deviations, _, _)| s.std > 0.0 && *deviations > sigma)
        .collect();
    spikes.sort_by(|a, b| b.0.total_cmp(&a.0));
    let total = spikes.len();
    spikes.truncate(count_arg(args, "limit", 20));
    Ok(json!({
        "entity": e.display_name(),
        "name": name,
        "mean": secs(s.mean),
        "std_dev": secs(s.std),
        "sigma": sigma,
        "total_spikes": total,
        "spikes": spikes.iter().map(|(dev, f, v)| json!({
            "frame": f,
            "time": secs(r.time_for_frame(*f)),
            "value": num(*v),
            "deviations": secs(*dev),
        })).collect::<Vec<_>>(),
    }))
}

/// Runs of one text parameter: consecutive samples with the same value merged.
fn runs(r: &Replay, e: &Entity, list: &FrameList<String>) -> Vec<(i32, i32, String)> {
    let end = r.lifetime(e.id).map_or(r.last_frame(), |l| l.1);
    let mut out: Vec<(i32, i32, String)> = Vec::new();
    for (frame, value) in list.iter() {
        match out.last_mut() {
            Some(last) if last.2 == *value => {}
            Some(last) => {
                last.1 = frame - 1;
                out.push((frame, end, value.clone()));
            }
            None => out.push((frame, end, value.clone())),
        }
    }
    out
}

fn get_param_timeline(r: &Replay, args: &Value) -> Outcome {
    let e = entity_arg(r, args)?;
    let (from, to) = frame_range(r, args);
    let Some(name) = str_arg(args, "name") else {
        let parameters: Vec<Value> = e
            .params
            .iter()
            .map(|(name, list)| {
                let all = runs(r, e, list);
                let distinct: BTreeSet<&String> = all.iter().map(|run| &run.2).collect();
                json!({
                    "name": name,
                    "runs": all.len(),
                    "distinct_values": distinct.len(),
                    "first_value": all.first().map(|run| run.2.clone()),
                    "last_value": all.last().map(|run| run.2.clone()),
                })
            })
            .collect();
        return Ok(json!({ "entity": e.display_name(), "parameters": parameters }));
    };
    let lower = name.to_lowercase();
    let (key, list) = e.params.iter().find(|(k, _)| k.to_lowercase() == lower).ok_or_else(|| {
        format!(
            "{} has no text parameter `{name}`. It has: {}",
            e.display_name(),
            e.params.keys().cloned().collect::<Vec<_>>().join(", ")
        )
    })?;
    let inside: Vec<(i32, i32, String)> =
        runs(r, e, list).into_iter().filter(|run| run.1 >= from && run.0 <= to).collect();
    let (shown, offset) = page(&inside, args, 200);
    Ok(json!({
        "entity": e.display_name(),
        "name": key,
        "total_runs": inside.len(),
        "offset": offset,
        "runs": shown.iter().map(|(a, b, v)| json!({
            "value": v,
            "first_frame": a,
            "last_frame": b,
            "start_time": secs(r.time_for_frame(*a)),
            "end_time": secs(if *b >= r.last_frame() { r.total_time() } else { r.time_for_frame(*b + 1) }),
        })).collect::<Vec<_>>(),
    }))
}

fn get_trajectory(r: &Replay, args: &Value) -> Outcome {
    let e = entity_arg(r, args)?;
    let (from, to) = frame_range(r, args);
    let samples: Vec<(i32, Point)> = e.transforms.range(from, to).map(|(f, t)| (f, t.translation)).collect();
    if samples.is_empty() {
        return Err(format!("{} has no recorded positions in frames {from} to {to}", e.display_name()).into());
    }
    let length =
        |a: Point, b: Point| f64::from(((b.x - a.x).powi(2) + (b.y - a.y).powi(2) + (b.z - a.z).powi(2)).sqrt());
    let distance: f64 = samples.windows(2).map(|w| length(w[0].1, w[1].1)).sum();
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for (_, p) in &samples {
        for (i, v) in [p.x, p.y, p.z].into_iter().enumerate() {
            lo[i] = lo[i].min(v);
            hi[i] = hi[i].max(v);
        }
    }
    let seconds = r.time_for_frame(samples[samples.len() - 1].0) - r.time_for_frame(samples[0].0);
    // The last sample is always kept, so the rest are spaced to leave room for it.
    let max = count_arg(args, "max_points", 100).max(2);
    let stride = samples.len().div_ceil(max - 1).max(1);
    let mut shown: Vec<&(i32, Point)> = samples.iter().step_by(stride).collect();
    if let Some(last) = samples.last()
        && shown.last().is_none_or(|l| l.0 != last.0)
    {
        shown.push(last);
    }
    Ok(json!({
        "entity": e.display_name(),
        "from_frame": from,
        "to_frame": to,
        "samples": samples.len(),
        "distance": secs(distance),
        "average_speed": if seconds > 0.0 { secs(distance / seconds) } else { Value::Null },
        "bounds": { "min": [num(lo[0]), num(lo[1]), num(lo[2])], "max": [num(hi[0]), num(hi[1]), num(hi[2])] },
        "points": shown.iter().map(|(f, p)| json!([f, num(p.x), num(p.y), num(p.z)])).collect::<Vec<_>>(),
    }))
}

fn get_frame(r: &Replay, args: &Value) -> Outcome {
    let frame = int_arg(args, "frame").ok_or("the `frame` argument is required")?;
    if frame < 0 || frame > r.last_frame() {
        return Err(format!("frame {frame} is outside the capture (0 to {})", r.last_frame()).into());
    }
    let limit = count_arg(args, "limit", 100);
    let alive: Vec<&Entity> = r.entities().filter(|e| r.is_alive_at(e.id, frame)).collect();
    let logs = r.logs_in(frame, frame);
    let mut draws = BTreeMap::<&str, usize>::new();
    for d in r.draws_at(frame) {
        *draws.entry(d.category.as_str()).or_default() += 1;
    }
    Ok(json!({
        "frame": frame,
        "time": secs(r.time_for_frame(frame)),
        "alive_entities": alive.len(),
        "entities": alive.iter().take(limit).map(|e| json!({
            "id": e.id,
            "name": e.display_name(),
            "category": e.category,
            "position": position_json(r, e, frame),
        })).collect::<Vec<_>>(),
        "log_entries": logs.len(),
        "logs": logs.iter().take(limit).map(|l| json!({
            "category": l.category,
            "entity_id": l.entity,
            "entity": r.entity(l.entity).map(Entity::display_name),
            "message": l.message,
        })).collect::<Vec<_>>(),
        "debug_draws_by_category": draws,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_path_argument_is_only_for_hosts_that_read_files() {
        for tool in definitions(true) {
            assert!(tool["inputSchema"]["properties"].get("path").is_some());
            assert!(tool["inputSchema"]["required"].as_array().unwrap().iter().any(|r| r == "path"));
        }
        for tool in definitions(false) {
            assert!(tool["inputSchema"]["properties"].get("path").is_none());
            assert!(tool["inputSchema"]["required"].as_array().unwrap().iter().all(|r| r != "path"));
        }
    }
}

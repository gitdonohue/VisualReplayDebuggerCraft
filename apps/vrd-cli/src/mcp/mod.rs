//! `vrd-cli mcp`: a Model Context Protocol server (JSON-RPC 2.0 over stdio, one message per line)
//! that lets an AI assistant read and analyse `.vrd` captures.

use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::time::SystemTime;

use serde_json::{Value, json};
use vrd_model::Replay;

pub const USAGE: &str = "\
    vrd-cli mcp [--root DIR]            Serve captures over MCP (stdio). Relative paths resolve against DIR
";

const SERVER_NAME: &str = "vrd-cli";
const LATEST_PROTOCOL: &str = "2025-06-18";
const SUPPORTED_PROTOCOLS: [&str; 4] = ["2025-06-18", "2025-03-26", "2024-11-05", "2024-10-07"];

pub use vrd_analysis::ToolError;

struct Loaded {
    modified: Option<SystemTime>,
    replay: Replay,
}

/// The server state: where relative paths start, and the captures read so far.
pub struct Server {
    root: Option<PathBuf>,
    captures: HashMap<PathBuf, Loaded>,
}

impl Server {
    pub fn new(root: Option<PathBuf>) -> Self {
        Self { root, captures: HashMap::new() }
    }

    /// The capture at `path`, read again if the file changed since it was last read.
    pub fn capture(&mut self, path: &str) -> Result<&Replay, ToolError> {
        let mut full = PathBuf::from(path);
        if full.is_relative()
            && let Some(root) = &self.root
        {
            full = root.join(full);
        }
        let modified = std::fs::metadata(&full).and_then(|m| m.modified()).ok();
        let stale = self.captures.get(&full).is_none_or(|l| l.modified != modified);
        if stale {
            let bytes = std::fs::read(&full).map_err(|e| format!("cannot read {}: {e}", full.display()))?;
            let replay = Replay::from_bytes(&bytes).map_err(|e| format!("{}: {e}", full.display()))?;
            self.captures.insert(full.clone(), Loaded { modified, replay });
        }
        Ok(&self.captures[&full].replay)
    }

    /// Answers one JSON-RPC message. Notifications (no `id`) get no answer.
    pub fn handle(&mut self, message: &Value) -> Option<Value> {
        let id = message.get("id").cloned();
        let method = message.get("method").and_then(Value::as_str);
        let Some(method) = method else {
            // A response to something we never asked, or garbage.
            return id.map(|id| error(id, -32600, "invalid request"));
        };
        let params = message.get("params").cloned().unwrap_or(Value::Null);
        let id = id?;

        let result = match method {
            "initialize" => Ok(initialize(&params)),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": vrd_analysis::definitions(true) })),
            "tools/call" => Ok(self.call(&params)),
            "resources/list" => Ok(json!({ "resources": [] })),
            "prompts/list" => Ok(json!({ "prompts": [] })),
            other => Err(error(id.clone(), -32601, &format!("method not found: {other}"))),
        };
        Some(match result {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Err(e) => e,
        })
    }

    fn call(&mut self, params: &Value) -> Value {
        let Some(name) = params.get("name").and_then(Value::as_str) else {
            return tool_result(Err("tools/call needs a tool name".into()));
        };
        let arguments = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
        let Some(path) = arguments.get("path").and_then(Value::as_str) else {
            return tool_result(Err("the `path` argument is required".into()));
        };
        let outcome = self.capture(path).and_then(|replay| vrd_analysis::call(replay, name, &arguments));
        tool_result(outcome)
    }
}

fn initialize(params: &Value) -> Value {
    let asked = params.get("protocolVersion").and_then(Value::as_str).unwrap_or(LATEST_PROTOCOL);
    let version = if SUPPORTED_PROTOCOLS.contains(&asked) { asked } else { LATEST_PROTOCOL };
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": { "listChanged": false } },
        "serverInfo": { "name": SERVER_NAME, "version": env!("CARGO_PKG_VERSION") },
        "instructions": "Read and analyse VisualReplayDebugger (.vrd) captures. Every tool takes the `path` of a \
            capture. Start with capture_info, then list_entities; entities can be named by id or by name. \
            Frames are the capture's unit of time; each frame also has a time in seconds."
    })
}

fn tool_result(outcome: Result<Value, ToolError>) -> Value {
    match outcome {
        Ok(value) => json!({
            "content": [{ "type": "text", "text": serde_json::to_string(&value).unwrap_or_default() }],
            "structuredContent": value,
            "isError": false,
        }),
        Err(ToolError(message)) => json!({
            "content": [{ "type": "text", "text": message }],
            "isError": true,
        }),
    }
}

fn error(id: Value, code: i32, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

pub fn run(args: &[String]) -> Result<(), String> {
    let root = match args.iter().position(|a| a == "--root") {
        Some(i) => Some(PathBuf::from(args.get(i + 1).ok_or("--root needs a folder")?)),
        None => None,
    };
    let mut server = Server::new(root);
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line.map_err(|e| e.to_string())?;
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(Value::Array(batch)) => {
                let replies: Vec<Value> = batch.iter().filter_map(|m| server.handle(m)).collect();
                (!replies.is_empty()).then_some(Value::Array(replies))
            }
            Ok(message) => server.handle(&message),
            Err(e) => Some(error(Value::Null, -32700, &format!("parse error: {e}"))),
        };
        if let Some(reply) = reply {
            writeln!(stdout, "{reply}").and_then(|()| stdout.flush()).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> String {
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/sample.vrd").to_string()
    }

    fn call(server: &mut Server, tool: &str, arguments: Value) -> Value {
        let reply = server
            .handle(&json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call",
                "params": { "name": tool, "arguments": arguments } }))
            .expect("a reply");
        reply["result"].clone()
    }

    fn ok(server: &mut Server, tool: &str, arguments: Value) -> Value {
        let result = call(server, tool, arguments);
        assert_eq!(result["isError"], json!(false), "{tool}: {result}");
        result["structuredContent"].clone()
    }

    #[test]
    fn it_introduces_itself_and_lists_its_tools() {
        let mut server = Server::new(None);
        let init = server
            .handle(&json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize",
                "params": { "protocolVersion": "2024-11-05" } }))
            .unwrap();
        assert_eq!(init["result"]["protocolVersion"], "2024-11-05");
        assert!(init["result"]["capabilities"]["tools"].is_object());

        assert!(server.handle(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })).is_none());

        let list = server.handle(&json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" })).unwrap();
        let tools = list["result"]["tools"].as_array().unwrap();
        assert!(tools.len() >= 8);
        for tool in tools {
            assert!(tool["name"].is_string() && tool["description"].is_string());
            assert_eq!(tool["inputSchema"]["type"], "object");
            assert!(tool["inputSchema"]["required"].as_array().unwrap().iter().any(|r| r == "path"));
        }

        let unknown = server.handle(&json!({ "jsonrpc": "2.0", "id": 3, "method": "nope" })).unwrap();
        assert_eq!(unknown["error"]["code"], -32601);
    }

    #[test]
    fn mistakes_are_reported_to_the_assistant_not_as_protocol_errors() {
        let mut server = Server::new(None);
        let missing = call(&mut server, "capture_info", json!({ "path": "no/such/file.vrd" }));
        assert_eq!(missing["isError"], json!(true));
        let unknown = call(&mut server, "no_such_tool", json!({ "path": sample() }));
        assert_eq!(unknown["isError"], json!(true));
        let no_path = call(&mut server, "capture_info", json!({}));
        assert_eq!(no_path["isError"], json!(true));
        let bad_entity = call(&mut server, "get_entity", json!({ "path": sample(), "entity": "nobody" }));
        assert_eq!(bad_entity["isError"], json!(true));
    }

    #[test]
    fn it_describes_the_sample_capture() {
        let mut server = Server::new(None);
        let info = ok(&mut server, "capture_info", json!({ "path": sample() }));
        assert_eq!(info["frames"], 1386);
        assert_eq!(info["entities"], 37);
        assert_eq!(info["logs"], 2559);

        let entities =
            ok(&mut server, "list_entities", json!({ "path": sample(), "filter": "stormtrooper", "limit": 3 }));
        assert!(entities["total"].as_u64().unwrap() >= 3);
        assert_eq!(entities["entities"].as_array().unwrap().len(), 3);

        let player =
            ok(&mut server, "get_entity", json!({ "path": sample(), "entity": "PlayerCharacter", "frame": 600 }));
        assert_eq!(player["name"], "PlayerCharacter");
        assert!(player["text_parameters"]["CharacterState"].is_string());
        // The same entity by id.
        let by_id = ok(&mut server, "get_entity", json!({ "path": sample(), "entity": player["id"].clone() }));
        assert_eq!(by_id["name"], "PlayerCharacter");
    }

    #[test]
    fn it_searches_and_summarises_logs() {
        let mut server = Server::new(None);
        let all = ok(&mut server, "search_logs", json!({ "path": sample(), "limit": 5 }));
        assert_eq!(all["total"], 2559);
        assert_eq!(all["entries"].as_array().unwrap().len(), 5);

        let some = ok(&mut server, "search_logs", json!({ "path": sample(), "text": "state changed", "limit": 1000 }));
        let total = some["total"].as_u64().unwrap();
        assert!(total > 0 && total < 2559);
        assert!(
            some["entries"].as_array().unwrap().iter().all(|e| e["message"]
                .as_str()
                .unwrap()
                .to_lowercase()
                .contains("state changed"))
        );

        let summary = ok(&mut server, "log_summary", json!({ "path": sample() }));
        let by_category: u64 =
            summary["by_category"].as_array().unwrap().iter().map(|c| c["count"].as_u64().unwrap()).sum();
        assert_eq!(by_category, 2559);
    }

    #[test]
    fn it_reads_series_parameters_and_positions() {
        let mut server = Server::new(None);
        let list = ok(&mut server, "get_series", json!({ "path": sample(), "entity": "SystemStats" }));
        let names: Vec<&str> = list["series"].as_array().unwrap().iter().map(|s| s["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"FPS"));

        let fps = ok(
            &mut server,
            "get_series",
            json!({ "path": sample(), "entity": "SystemStats", "name": "FPS", "max_points": 50 }),
        );
        assert!(fps["points"].as_array().unwrap().len() <= 50);
        assert!(fps["stats"]["min"].as_f64().unwrap() <= fps["stats"]["max"].as_f64().unwrap());

        let spikes = ok(
            &mut server,
            "find_spikes",
            json!({ "path": sample(), "entity": "SystemStats", "name": "timestepMS", "sigma": 2.0 }),
        );
        assert!(spikes["spikes"].is_array());

        let changes = ok(
            &mut server,
            "get_param_timeline",
            json!({ "path": sample(), "entity": "PlayerCharacter", "name": "MoveState" }),
        );
        assert!(!changes["runs"].as_array().unwrap().is_empty());
        let overview = ok(&mut server, "get_param_timeline", json!({ "path": sample(), "entity": "PlayerCharacter" }));
        assert_eq!(overview["parameters"].as_array().unwrap().len(), 2);

        let path = ok(
            &mut server,
            "get_trajectory",
            json!({ "path": sample(), "entity": "PlayerCharacter", "max_points": 20 }),
        );
        assert!(path["points"].as_array().unwrap().len() <= 20);
        assert!(path["distance"].as_f64().unwrap() >= 0.0);

        let frame = ok(&mut server, "get_frame", json!({ "path": sample(), "frame": 600 }));
        assert_eq!(frame["frame"], 600);
        assert!(!frame["entities"].as_array().unwrap().is_empty());
    }
}

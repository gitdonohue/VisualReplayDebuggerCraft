# WebMCP: agent tools in the browser

The web version offers its tools to AI agents through [WebMCP](https://webmachinelearning.github.io/webmcp/),
the browser API (`document.modelContext`) that lets a page register tools for an agent built into or attached
to the browser. A user opens the debugger, loads a capture, and asks their browser's assistant about it; the
assistant works with the capture and the view the user is looking at.

It is the same set of questions as the [MCP server](mcp.md) (they share `crates/analysis`), minus the `path`
argument: they act on the capture that is open. On top of that there are tools that drive the view, so the
agent and the user share one screen.

## Tools

Questions about the open capture (read-only): `capture_info`, `list_entities`, `get_entity`, `search_logs`,
`log_summary`, `get_series`, `find_spikes`, `get_param_timeline`, `get_trajectory`, `get_frame`. See
[mcp.md](mcp.md) for what each returns.

Tools that act on the view:

| Tool | What it does |
|---|---|
| `get_view_state` | The capture, the frame and time, whether it is playing, the selected entity, the visible panel and the time span shown |
| `seek` | Move the playhead to a `frame` or a `time` (seconds); pauses playback |
| `set_playback` | Start or stop playback and/or set its `speed` |
| `select_entity` | Select an entity (by id or name) and, by default, point the 3D camera at it |
| `show_panel` | Bring `logs`, `graphs`, `timelines` or `entity_timelines` to the front |
| `toggle_pin` | Pin an entity in the graphs and timelines, or unpin it |
| `zoom_time` | Show a span of time in the graphs and timelines, or everything without arguments |

Without an open capture, the tools say so (`No capture is open`). Results come back as MCP-style text content,
with `isError` set when a call failed and the message listing what would have worked.

## Browser support

WebMCP is a draft and only some browsers implement it, some behind a flag. The app looks for
`document.modelContext` (the current draft) and falls back to `navigator.modelContext` (earlier previews). If
neither exists it logs one line to the console and carries on; nothing else changes. When tools are
registered the console says `WebMCP: registered 17 tools`.

## How it fits together

* `crates/analysis` holds the questions: JSON in, JSON out, no file or browser code.
* `crates/ui-egui/src/bridge.rs`: `Bridge` is the app's side. It holds the open capture, queues commands from
  agents to run at the start of the next frame, and keeps a snapshot of what is on screen. It has no browser
  code, so it is tested natively (`crates/ui-egui/tests/bridge.rs`).
* `apps/vrd-web/src/webmcp.rs` is the only JavaScript glue: it registers each tool with the browser.

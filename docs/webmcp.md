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

## Using it

Open the [web version](https://gitdonohue.github.io/VisualReplayDebuggerCraft/?url=sample.vrd) (or your own
build) in a browser that supports WebMCP and load a capture. Then use whatever agent your browser provides, or
connect a coding agent as described below. Things to ask:

* "Summarise what happened in this capture." (`capture_info`, `log_summary`)
* "When did the frame time spike, and what was going on?" (`find_spikes`, `get_frame`, `search_logs`)
* "Why did PlayerCharacter change state around 24 s?" (`get_param_timeline`, `search_logs`)
* "Show me where imperial_stormtrooper#1 despawns." (`select_entity`, `show_panel`, `seek`)

### See the tools in the browser

Browsers that implement WebMCP list a page's tools in their own tooling. In Chrome, the *Model Context Tool
Inspector* shows the 17 tools of this app, with their descriptions and schemas, and can run them. The page's
console also logs `WebMCP: registered 17 tools` at start-up, or `WebMCP is not available in this browser` if
the browser has no WebMCP.

### From a coding agent (Chrome DevTools MCP)

A coding agent such as OpenCode, Claude Code or Gemini CLI can use the tools of a page through the
[Chrome DevTools MCP](https://github.com/ChromeDevTools/chrome-devtools-mcp) server. Its WebMCP tools are off
by default; turn them on and point it at the Chrome that has the debugger open:

```json
{
  "command": "npx",
  "args": ["-y", "chrome-devtools-mcp@latest", "--autoConnect", "--categoryExperimentalWebmcp"]
}
```

(the exact place in the config depends on the agent; in OpenCode it is the `chrome-devtools-mcp` entry under
`mcp`). Then:

1. In Chrome, open `chrome://inspect/#remote-debugging` and enable remote debugging.
2. Restart the agent's MCP servers and keep the debugger tab open. Chrome asks you to allow the connection.
3. The agent sees the tab in `list_pages`, with the page's WebMCP tools listed under it. It runs them with
   `list_webmcp_tools` and `execute_webmcp_tool` (`toolName`, and `input` as a JSON string).

Notes from testing:

* `--autoConnect` gives the agent access to **every tab in your Chrome profile**, including logged-in
  sessions. To keep that separate, start a dedicated Chrome with
  `--remote-debugging-port=9222 --user-data-dir=<a new folder>` and use `--browser-url=http://127.0.0.1:9222`
  instead of `--autoConnect`.
* Without `--autoConnect` or `--browser-url`, the server starts its own Chrome. A stock Chrome started that
  way did not expose WebMCP, so the tools did not register. Attach to a browser that has it.
* The agent changes your view when it calls `seek`, `select_entity`, `show_panel`, `toggle_pin`,
  `set_playback` or `zoom_time`. The question tools never do.

### Trying it without WebMCP support

To test the tools in a browser that has no WebMCP, define a stand-in before the app starts (for example with
DevTools snippets, or Chrome DevTools MCP's `initScript` when navigating) and call the tools yourself:

```js
// Run before the page's own scripts.
window.__tools = {};
Object.defineProperty(document, 'modelContext', {
  value: { registerTool(tool) { window.__tools[tool.name] = tool; return Promise.resolve(); } },
});

// After the capture has loaded:
const reply = await window.__tools.search_logs.execute({ text: 'state changed', limit: 3 });
console.log(JSON.parse(reply.content[0].text));
```
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

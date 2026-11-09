# The MCP server

`vrd-cli mcp` is a [Model Context Protocol](https://modelcontextprotocol.io) server over stdio (one JSON-RPC
message per line), so an AI assistant can read and analyse `.vrd` captures. It only reads captures, and a
capture is read again if the file changes.

```
vrd-cli mcp [--root DIR]     # relative capture paths resolve against DIR
```

## Using it

Register it with your MCP client. For example, in a Claude Desktop or similar `mcpServers` section:

```json
{
  "mcpServers": {
    "vrd": {
      "command": "cargo",
      "args": ["run", "--release", "-q", "-p", "vrd-cli", "--", "mcp", "--root", "C:/captures"]
    }
  }
}
```

(or point `command` at a built `vrd-cli` binary). Then ask things like "what happened around frame 600?",
"which entity logged the most?" or "when does timestepMS spike?".

## Tools

Every tool takes `path`, the capture to read. Entities are named by id or by name (a unique part of the name or
path is enough; an ambiguous one gets a list of candidates). Frames are the capture's unit of time and each
comes with its time in seconds. Lists are paged with `limit` and `offset`.

| Tool | What it answers |
|---|---|
| `capture_info` | Frames, duration, entity/log/draw counts, categories, parameter names, whether it is truncated |
| `list_entities` | Entities with lifetime, position at a frame, and how much data each has; filter by text or category |
| `get_entity` | One entity at a frame: position, numeric and text parameters, static parameters, children |
| `search_logs` | Log entries by text, category, entity and frame range |
| `log_summary` | Log counts by category and entity, and the busiest frames |
| `get_series` | Numeric parameters: statistics for all, or one series reduced to `max_points` keeping its extremes |
| `find_spikes` | Values more than `sigma` standard deviations from the mean of a series |
| `get_param_timeline` | Text parameters (states): how often they change, or their runs with frames and times |
| `get_trajectory` | Positions over time, distance travelled, bounds and average speed |
| `get_frame` | A snapshot: who exists and where, the logs written on that frame, draws per category |

Mistakes (a missing file, an unknown entity or series) come back as tool errors with the choices that exist, so
the assistant can correct itself. Results are also returned as `structuredContent`.

---
name: analyse-vrd-capture
description: Use when asked to investigate, summarise or debug a VisualReplayDebugger capture (.vrd file) - what happened in a recording of a game or AI session, why an entity behaved a certain way, when something spiked or changed state, or where something moved. Explains how to use the vrd MCP tools (or the vrd-cli commands) to answer such questions efficiently.
---

# Analysing a `.vrd` capture

A `.vrd` capture is a binary recording of a game session: entities with positions over time, numeric
parameters (FPS, health, ...), text parameters (states, modes), log entries and debug draws, all indexed by
**frame**. You cannot read the file directly. Use the `vrd` MCP server's tools (`vrd-cli mcp`, see
`docs/mcp.md`); every tool takes the capture's `path`.

If the MCP tools are not available, fall back to the CLI, which prints text: `vrd-cli info <file>`,
`vrd-cli logs <file> --limit N`, `vrd-cli entities <file> --frame N`. It has no filtering, so pipe through a
search tool and keep the output small.

## Working method

1. **Orient first.** `capture_info` (size, duration, categories, parameter names), then `log_summary`
   (what is chatty, which entities, busiest frames). Do not start by listing everything.
2. **Narrow before you read.** Use `list_entities` with `filter`/`category`/`with_logs`, and `search_logs` with
   `text`, `category`, `entity` and a frame range. Lists are paged: check `total` and use `offset`.
3. **Go from a frame to a story.** Once you have a frame of interest, `get_frame` for the world at that
   moment, `get_entity` for one entity's parameters then, and `search_logs` with `from_frame`/`to_frame`
   a little either side.
4. **Quote frames and times.** Always report both (the tools return both) so the user can jump there in the
   viewer with the same frame number.

Entities are named by id or by name; a unique part of the name is enough. If a name is ambiguous the error
lists the candidates: use an id from it. Entity names often repeat with `#N` suffixes
(`imperial_stormtrooper#1`).

## Playbooks

**"What happened?" / summarise a capture.** `capture_info` -> `log_summary` -> `list_entities` with
`with_logs: true` -> for the top entities `get_param_timeline` (no `name`) to see which states they went
through -> `search_logs` around the busiest frames. Report in time order.

**"Why did X do Y?" (AI/behaviour).** Find X (`list_entities` with `filter`). `get_param_timeline` for X without
`name` lists its state parameters and how often they change; with `name` it gives the runs, so you can see
what state X was in when Y happened. Then `search_logs` with `entity: X` over the frames around the change:
the log messages usually give the reason ("State changed: A -> B", "Scope started ...").
Compare against the other entities' states at the same frame with `get_frame`.

**Performance hitches.** `get_series` on the system/stats entity (no `name` lists the series with min/max/mean:
look for frame time, FPS, draw calls, triangles). Then `find_spikes` on the suspect series (`sigma` 3 is a
good start; lower it if nothing comes back). For each spike frame run `get_frame` and `search_logs` for that
frame to see what else was happening (spawns, many entities, heavy logging).

**Movement.** `get_trajectory` gives distance, average speed, bounds and a sampled path. Compare two entities
by running it for both over the same frame range. A big jump between consecutive points can mean a teleport
or a respawn: check `search_logs` there.

**Spawns and despawns.** `list_entities` has each entity's `first_frame`/`last_frame`; use `alive_only` with a
frame to see who existed then. Bursts of creation show up as many entities with the same `first_frame`.

## Cautions

- Positions are in the game's own units and axes: the capture does not say which axis is up or what a unit
  is. Do not assume metres or Z-up. Say "units".
- A capture can be **truncated** (still being recorded); `capture_info` says so. Results then cover only what
  was written.
- Numeric series are sampled when the game recorded them, not every frame. Do not treat gaps as zeros.
- Tool results are capped (`limit` up to 1000, `max_points` decimated). If `total` is larger than what came
  back, say the answer is partial or page further.
- Log text is whatever the game wrote. Quote it exactly rather than paraphrasing when it matters.

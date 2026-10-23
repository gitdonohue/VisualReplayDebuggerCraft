# The `.vrd` capture format

This describes what `vrd-format` reads and writes. It is derived from the original C#/C serializers and
reader in [VisualReplayDebugger](https://github.com/gitdonohue/VisualReplayDebugger), and existing captures
must keep loading.

## Container

* If the first four bytes are the little-endian int32 `0xFF`, the rest is an **uncompressed** block stream.
* Otherwise the whole file is a **raw deflate** stream (no zlib header) of blocks. `0xFF` can never start a
  valid deflate stream, so the two cases are unambiguous.
* A stream never has to be closed. A capture that stops mid-block is valid up to its last complete block.
  `vrd-format` reports this as `truncated` rather than failing.

## Primitives

| Type | Encoding |
|---|---|
| varint | .NET 7-bit encoded int32: 7 bits per byte, low group first, high bit means "more" (max 5 bytes) |
| f32 / i32 | 4 bytes little-endian (`i32` is only used for the mesh vertex count) |
| string | varint byte length, then that many bytes (ASCII) |
| point | 3 x f32 |
| transform | point translation, then 4 x f32 quaternion (x, y, z, w) |
| color | varint index into the `System.Windows.Media.Colors` order (see `crates/format/src/color.rs`) |

## Blocks

Each block starts with a varint block type.

| Id | Name | Body |
|---|---|---|
| 1 | FrameStep | f32 total time |
| 2 | EntityDef | frame, id, then the entity definition |
| 3 | EntityUndef | frame, id |
| 4 | EntitySetPos | frame, id, point |
| 5 | EntitySetTransform | frame, id, transform |
| 6 | EntityLog | frame, id, string category, string message, color |
| 7 | EntityParameter | frame, id, string label, string value |
| 8 | EntityValue | frame, id, string label, f32 value |
| 9 | EntityLine | frame, id, string category, point start, point end, color |
| 10 | EntityCircle | frame, id, string category, point position, point up, f32 radius, color |
| 11 | EntitySphere | frame, id, string category, point center, f32 radius, color |
| 12 | EntityCapsule | frame, id, string category, point p1, point p2, f32 radius, color |
| 13 | EntityMesh | frame, id, string category, i32 count, count x point, color |
| 14 | EntityBox | frame, id, string category, transform, point dimensions, color |
| 15 | EntityDefWithParent | frame, id, varint parent id, then the entity definition |
| 255 | ReplayHeader | empty marker, written first in compressed streams by the C serializer |

`frame`, `id` and the other varints above are varints. An **entity definition** is: varint id, string name,
string path, string type name, string category, transform, varint count followed by count x (string key,
string value) static parameters, varint creation frame.

## Time and frames

Frame 0 starts at time 0. Each FrameStep closes the current frame and gives the time at which the next one
starts, so frame `f` spans `frame_times[f] .. frame_times[f + 1]`. Blocks carry the frame they happened in.

## Known quirks

* The original .NET writer emits Box draws with block type 13 (Mesh) and the C writer emits them as type 8
  (Value). Both are writer bugs that produce unreadable data. The reader here follows the original *reader*,
  which expects the layout in the table above.
* Mesh draws are persistent only when their category is empty and they happen on the entity's registration
  frame. Everything else lasts for the single frame it was issued on.

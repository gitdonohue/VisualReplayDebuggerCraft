# The 3D viewport

`vrd-scene` builds what to draw from a `Replay` at a frame, `vrd-render` draws it with wgpu into an
offscreen texture, and `vrd-ui-egui` shows that texture and handles the mouse.

## What is drawn

| Thing | Source |
|---|---|
| Entity geometry (spheres, capsules, boxes, meshes) | Draws with an **empty category** on the entity's registration frame ("creation draws"). They follow the entity's transform. |
| Entity lines and circles | Creation draws of line/circle type, transformed by the entity. |
| Debug primitives | All other draws, on the frame they were issued (optionally the last N frames). Spheres, capsules, boxes and meshes are solid; lines and circles are lines. |
| Circles, axes, names, paths | Per entity that has transforms. Names and the selected entity's full path are shown for the selection; "Names" / "Paths" show all. |
| Grid | On the ground plane, spacing follows the camera. |

Filters: draw categories, draw colours, per-entity hide (right-click an entity). "Ghost" renders draws
semi-transparent.

## Conventions are detected, not assumed

The capture format has no units or axes. The original tool assumed Z-up, metres = 100, and cameras that face -X.
The sample capture is Y-up, in metres, and its camera looks along local -Z. So:

* **Up axis**: the axis the recorded positions vary least along (characters walk on a ground plane). Ties go
  to Z. Override in View > Up axis.
* **Camera forward** for `maincamera` follow: -X for Z-up, -Z otherwise. Override in View > Camera looks along.
* **Unit**: the median size of entity geometry (or a twentieth of the position spread). It scales default axis
  length, focus distance and the minimum selection radius.
* **Ground height**: the origin, unless the positions are well away from it, then the lowest position.
* **Registration geometry space**: captures differ on whether creation draws are relative to the entity or in
  world space at registration. If the geometry sits around the entity's initial position and not around the
  origin it is treated as world space and converted to entity space, so it moves with the entity.

## Controls

Left-drag orbits, right/middle-drag pans, scroll zooms, click selects, double-click (or F) focuses.
Moving the camera by hand stops "Camera" (follow the main camera).

## Rendering notes

Reversed-Z infinite projection, 4x MSAA, lines drawn as screen-space quads (anti-aliased, clipped against the
near plane in the vertex shader), two-sided lighting from the triangle winding (so winding must agree with
normals; there is a test for every primitive). Output is `Rgba8Unorm` holding sRGB-encoded colour.

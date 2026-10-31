# Parameter timelines

The Timelines tab (under the 3D view, `T`) shows the text parameters
(`SetDynamicParam(obj, key, "text")`) an entity recorded, as rows of coloured blocks over time. It is the
counterpart of the graphs, which show numbers. It shows the selected entity plus any pinned ones (up to 10,
shared with the graphs).

## Reading it

* A block is a stretch of time during which a value was constant. Its colour comes from its label, so equal
  labels have equal colours everywhere.
* The label and colour use only the part of a value before the first `|`; what follows is detail, shown on hover
  (a change in detail does not start a new block at depth rows, but does in the flat view).
* **By depth** (default) gives each level of a path its own row: `Game.Movement.States.Idle` becomes four rows,
  `Game`, `Movement`, `States`, `Idle`. Neighbouring values that share a level merge into one block, so the top
  rows show the big phases and the bottom row the detail. A child under a different parent is a different block
  even when its label matches. Off: one row per parameter showing whole values.
* **Changes only** hides rows whose value never changes (a top level that is always `Game` is noise).
* Names that share a long prefix (`#24579765: imperial_stormtrooper#1.Slot: Attack...`) are shown without it; hover
  for the full name. The same shortening is used in the Properties panel.
* A parameter set to the empty string leaves a gap. Rows are shaded before the entity exists and after it is
  unregistered. Alternate frames are shaded when you are zoomed in far enough to tell them apart.
* Parameters that only ever clear themselves (the sample's `CharacterAIController` entities) are not listed.

## Busy rows

A row with more changes than pixels is not drawn one block per change. Runs narrower than 3 px are gathered into a
**busy block** labelled with its change count (`283×`) and tinted by the value that held longest. Hover for the
span and double-click to zoom into it. Wide runs next to a busy stretch stay separate blocks. The cost is
bounded by the width, never by the number of changes (`vrd-graph::blocks`).

## Time and navigation

The time axis works as in the graphs (click to seek, drag the red playhead to scrub, drag elsewhere to pan,
scroll over the blocks to zoom about the pointer, "Fit" to show everything). Scrolling over the row labels
scrolls the rows instead. Extra:

* **Double-click a block** zooms to it, with a little room around it. Double-click empty space fits everything.
* **◀ Event / Event ▶** moves the playhead to the previous or next start or end of a block among the rows shown.
* **Zoom block** zooms to the shortest block under the playhead.
* **Parameters** hides parameters by name; the filter box matches names (`a|b`).

The view follows the playhead during playback as in the graphs, and "Follow" is one setting for both tabs.

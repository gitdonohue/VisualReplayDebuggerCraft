# Graphs

The Graphs tab (under the 3D view, `G`) plots every numeric parameter (`SetDynamicParam(obj, key, float)`)
an entity recorded, against time. It shows the selected entity plus any pinned ones (up to 10).

## Reading it

* Each value holds until the next one: graphs are step functions, as the data is.
* A series starts at its first value and stops when its entity is unregistered.
* Every band has its own vertical scale, from zero (or the lowest negative value) to the highest value, and at
  least 0..1 so that small scores such as utilities keep a common scale. The top and bottom are labelled.
* "Clip spikes" scales to the typical range: a maximum more than three standard deviations above the mean is
  clipped at two, as the original tool's "autoscale" did.
* At the playhead each series shows its value. Hovering shows the time, the frame and every value in that band.

## Layout

| Toggle | Effect |
|---|---|
| Per entity | Each entity gets its own block, sized by how many bands it holds. Off: entities share bands, so the same parameter from several entities is overlaid and coloured by entity (see the legend). |
| Per parameter | Each parameter gets its own band. Off: all of an entity's parameters share one band. |
| By depth | Parameters with nested names (`Physics.ByType.Raycast`) are banded by depth, which groups related values such as utility scores. Takes precedence over "per parameter" when names are nested. |

Colours follow parameter names, so the same name has the same colour everywhere.

## Time

Click to seek; drag the red playhead to scrub; drag elsewhere to pan; scroll to zoom about the pointer;
double-click or "Fit" to show the whole capture. With "Follow" on, the view slides along during playback and
re-centres when the playhead jumps out of view; a paused playhead never moves the view.

## Performance

Each series is drawn exactly when it has at most one step per two pixels. Busier series are reduced per pixel
column to the column's entry, extremes and exit values, so the cost is bounded by the width and spikes are
never lost however far out you zoom (`vrd-graph::trace`).

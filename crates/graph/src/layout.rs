use std::collections::BTreeMap;

/// How the graphs split their height.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stacking {
    /// Each entity gets its own block. Otherwise the entities share one, and series with the same
    /// name (or depth) from different entities are drawn over each other for comparison.
    pub by_entity: bool,
    /// Each series name gets its own band.
    pub by_parameter: bool,
    /// Series are banded by how deeply their names are nested (`a.b.c`), which groups related
    /// values such as utility scores. Takes precedence over `by_parameter` when names are nested.
    pub by_depth: bool,
}

impl Default for Stacking {
    fn default() -> Self {
        Self { by_entity: true, by_parameter: true, by_depth: true }
    }
}

/// One series to draw: the graphed entity (by position in the list given to [`layout`]) and the
/// index of the series within that entity's series.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Member {
    pub entity: usize,
    pub series: usize,
}

/// A horizontal strip that some series share, with one vertical scale.
#[derive(Debug, Clone, PartialEq)]
pub struct Band {
    /// Top and bottom as fractions of the graph height.
    pub top: f32,
    pub bottom: f32,
    pub members: Vec<Member>,
    pub title: String,
    /// The band holds more than one entity, so its series should be told apart by entity.
    pub mixes_entities: bool,
}

/// The part of the height given to one entity.
#[derive(Debug, Clone, PartialEq)]
pub struct EntityBlock {
    pub entity: usize,
    pub top: f32,
    pub bottom: f32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Layout {
    /// Empty unless stacking by entity.
    pub blocks: Vec<EntityBlock>,
    pub bands: Vec<Band>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Key {
    Depth(usize),
    Name(String),
    All,
}

/// Splits the height between the graphed entities and their series.
///
/// `entities[i]` lists the series of graphed entity `i` that should be drawn, as
/// `(series index, name, depth)`. Entities with nothing to draw get no space.
pub fn layout(entities: &[Vec<(usize, &str, usize)>], stacking: Stacking) -> Layout {
    let present: Vec<usize> = (0..entities.len()).filter(|&i| !entities[i].is_empty()).collect();
    let mut out = Layout::default();
    if present.is_empty() {
        return out;
    }
    if stacking.by_entity {
        // Every band gets the same height, so an entity's block is as tall as its bands need.
        let per_entity: Vec<(usize, Vec<Band>)> =
            present.iter().map(|&e| (e, bands(entities, &[e], 0.0, 1.0, stacking))).collect();
        let total = per_entity.iter().map(|(_, b)| b.len()).sum::<usize>() as f32;
        let mut y = 0.0;
        for (entity, entity_bands) in per_entity {
            let height = entity_bands.len() as f32 / total;
            let (top, bottom) = (y, y + height);
            out.blocks.push(EntityBlock { entity, top, bottom });
            out.bands.extend(entity_bands.into_iter().map(|mut b| {
                b.top = top + b.top * height;
                b.bottom = top + b.bottom * height;
                b
            }));
            y = bottom;
        }
        if let (Some(block), Some(band)) = (out.blocks.last_mut(), out.bands.last_mut()) {
            block.bottom = 1.0;
            band.bottom = 1.0;
        }
    } else {
        out.bands = bands(entities, &present, 0.0, 1.0, stacking);
    }
    out
}

fn bands(
    entities: &[Vec<(usize, &str, usize)>],
    which: &[usize],
    top: f32,
    bottom: f32,
    stacking: Stacking,
) -> Vec<Band> {
    let all = || which.iter().flat_map(|&e| entities[e].iter().map(move |&(s, name, depth)| (e, s, name, depth)));
    let nested = stacking.by_depth && all().any(|(_, _, _, depth)| depth > 0);

    let mut groups: BTreeMap<Key, Vec<(Member, &str)>> = BTreeMap::new();
    for (entity, series, name, depth) in all() {
        let key = if nested {
            Key::Depth(depth)
        } else if stacking.by_parameter {
            Key::Name(name.to_string())
        } else {
            Key::All
        };
        groups.entry(key).or_default().push((Member { entity, series }, name));
    }

    let n = groups.len() as f32;
    let height = bottom - top;
    groups
        .into_iter()
        .enumerate()
        .map(|(k, (key, mut members))| {
            members.sort();
            let mut names: Vec<&str> = members.iter().map(|(_, name)| *name).collect();
            names.sort_unstable();
            names.dedup();
            let title = match (&key, names.as_slice()) {
                (_, [only]) => (*only).to_string(),
                (Key::Depth(d), _) => format!("Level {d} ({} series)", names.len()),
                _ => format!("{} series", names.len()),
            };
            let mixes_entities = members.iter().any(|(m, _)| m.entity != members[0].0.entity);
            Band {
                top: top + height * k as f32 / n,
                bottom: top + height * (k + 1) as f32 / n,
                members: members.into_iter().map(|(m, _)| m).collect(),
                title,
                mixes_entities,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stats_like() -> Vec<(usize, &'static str, usize)> {
        vec![(0, "DrawCalls", 0), (1, "FPS", 0), (2, "GPUTime", 0)]
    }

    fn physics_like() -> Vec<(usize, &'static str, usize)> {
        vec![
            (0, "Physics_Queries", 0),
            (1, "Physics_QueriesByType.Raycast", 1),
            (2, "Physics_QueriesByType.SphereSweep", 1),
        ]
    }

    fn stacking(by_entity: bool, by_parameter: bool, by_depth: bool) -> Stacking {
        Stacking { by_entity, by_parameter, by_depth }
    }

    fn spans_cover(bands: &[Band], top: f32, bottom: f32) {
        assert!((bands.first().unwrap().top - top).abs() < 1e-6);
        assert!((bands.last().unwrap().bottom - bottom).abs() < 1e-6);
        for w in bands.windows(2) {
            assert!((w[0].bottom - w[1].top).abs() < 1e-6, "bands tile without gaps");
        }
    }

    #[test]
    fn default_stacking_gives_each_entity_a_block_and_each_series_a_band() {
        let l = layout(&[stats_like()], Stacking::default());
        assert_eq!(l.blocks.len(), 1);
        assert_eq!(l.bands.len(), 3);
        assert_eq!(l.bands[1].title, "FPS");
        assert_eq!(l.bands[1].members, [Member { entity: 0, series: 1 }]);
        spans_cover(&l.bands, 0.0, 1.0);
    }

    #[test]
    fn nested_names_are_banded_by_depth() {
        let l = layout(&[physics_like()], Stacking::default());
        assert_eq!(l.bands.len(), 2);
        assert_eq!(l.bands[0].title, "Physics_Queries");
        assert_eq!(l.bands[1].members.len(), 2);
        assert_eq!(l.bands[1].title, "Level 1 (2 series)");
        // Without depth stacking every series gets a band again.
        assert_eq!(layout(&[physics_like()], stacking(true, true, false)).bands.len(), 3);
    }

    #[test]
    fn unstacked_parameters_share_one_band_per_entity() {
        let l = layout(&[stats_like(), physics_like()], stacking(true, false, false));
        assert_eq!(l.blocks.len(), 2);
        assert_eq!(l.bands.len(), 2);
        assert_eq!(l.bands[0].title, "3 series");
        assert_eq!((l.blocks[1].top, l.blocks[1].bottom), (0.5, 1.0));
        assert!(l.bands.iter().all(|b| !b.mixes_entities));
    }

    #[test]
    fn without_entity_blocks_same_named_series_are_overlaid_across_entities() {
        let a = vec![(0, "health", 0), (1, "speed", 0)];
        let b = vec![(0, "health", 0)];
        let l = layout(&[a, b], stacking(false, true, true));
        assert!(l.blocks.is_empty());
        assert_eq!(l.bands.len(), 2);
        let health = &l.bands[0];
        assert_eq!(health.title, "health");
        assert_eq!(health.members, [Member { entity: 0, series: 0 }, Member { entity: 1, series: 0 }]);
        assert!(health.mixes_entities);
        assert!(!l.bands[1].mixes_entities);
        spans_cover(&l.bands, 0.0, 1.0);
    }

    #[test]
    fn everything_in_one_band() {
        let l = layout(&[stats_like(), physics_like()], stacking(false, false, false));
        assert_eq!(l.bands.len(), 1);
        assert_eq!(l.bands[0].members.len(), 6);
        assert_eq!(l.bands[0].title, "6 series");
    }

    #[test]
    fn blocks_are_sized_by_how_many_bands_they_hold() {
        // Three bands for the stats, two (by depth) for physics: 3/5 and 2/5 of the height.
        let l = layout(&[stats_like(), physics_like()], Stacking::default());
        assert_eq!(l.blocks.len(), 2);
        assert!((l.blocks[0].bottom - 0.6).abs() < 1e-6, "{:?}", l.blocks);
        assert_eq!((l.blocks[1].top, l.blocks[1].bottom), (l.blocks[0].bottom, 1.0));
        spans_cover(&l.bands, 0.0, 1.0);
        let heights: Vec<f32> = l.bands.iter().map(|b| b.bottom - b.top).collect();
        assert!(heights.iter().all(|h| (h - 0.2).abs() < 1e-6), "equal bands: {heights:?}");
    }

    #[test]
    fn entities_with_nothing_to_draw_take_no_space() {
        let l = layout(&[vec![], stats_like(), vec![]], Stacking::default());
        assert_eq!(l.blocks.len(), 1);
        assert_eq!(l.blocks[0].entity, 1);
        assert_eq!((l.blocks[0].top, l.blocks[0].bottom), (0.0, 1.0));
        assert_eq!(layout(&[vec![], vec![]], Stacking::default()), Layout::default());
        assert_eq!(layout(&[], Stacking::default()), Layout::default());
    }
}

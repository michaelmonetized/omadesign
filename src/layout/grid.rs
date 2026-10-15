//! Bounded grid placement and track sizing shared by measurement and arrangement.
use super::{AutoStack, GridPlacement, GridTrack, GridTrackSize};
use crate::geom::Pt;

pub(super) const MAX_TRACKS: usize = 1024;
const MAX_SIZE: f32 = 1_000_000.0;

#[derive(Clone, Copy)]
pub(super) struct Item {
    pub size: Pt,
    pub placement: GridPlacement,
    pub fill_height: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Cell {
    pub column: usize,
    pub row: usize,
    pub columns: usize,
    pub rows: usize,
}

pub(super) struct Grid {
    pub cells: Vec<Cell>,
    pub columns: Vec<f32>,
    pub rows: Vec<f32>,
}

fn pixels(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, MAX_SIZE)
    } else {
        0.0
    }
}

fn span(value: u32) -> usize {
    (value as usize).clamp(1, MAX_TRACKS)
}

fn anchor(value: Option<u32>) -> Option<usize> {
    value.map(|v| (v as usize).min(MAX_TRACKS - 1))
}

struct Occupied {
    bits: Vec<u64>,
    words: usize,
}

impl Occupied {
    fn new(columns: usize) -> Self {
        let words = columns.div_ceil(64);
        Self {
            bits: vec![0; words * MAX_TRACKS],
            words,
        }
    }
    fn masks(&self, cell: Cell, mut visit: impl FnMut(usize, u64) -> bool) -> bool {
        let end = cell.column + cell.columns;
        for row in cell.row..cell.row + cell.rows {
            for word in cell.column / 64..end.div_ceil(64) {
                let lo = cell.column.saturating_sub(word * 64).min(64);
                let hi = end.saturating_sub(word * 64).min(64);
                let mask = (u64::MAX << lo) & (u64::MAX >> (64 - hi));
                if !visit(row * self.words + word, mask) {
                    return false;
                }
            }
        }
        true
    }
    fn free(&self, cell: Cell) -> bool {
        self.masks(cell, |index, mask| self.bits[index] & mask == 0)
    }
    fn insert(&mut self, cell: Cell) {
        let mut masks = Vec::new();
        self.masks(cell, |index, mask| {
            masks.push((index, mask));
            true
        });
        for (index, mask) in masks {
            self.bits[index] |= mask;
        }
    }
}

fn place(stack: &AutoStack, items: &[Item]) -> (Vec<Cell>, usize, usize) {
    let mut columns = if stack.column_tracks.is_empty() {
        (stack.columns as usize).clamp(1, MAX_TRACKS)
    } else {
        stack.column_tracks.len().min(MAX_TRACKS)
    };
    for item in items {
        let start = anchor(item.placement.column).unwrap_or(0);
        columns = columns.max((start + span(item.placement.column_span)).min(MAX_TRACKS));
    }
    let mut occupied = Occupied::new(columns);
    let mut cells: Vec<_> = items
        .iter()
        .map(|item| {
            let column = anchor(item.placement.column).unwrap_or(0);
            let row = anchor(item.placement.row).unwrap_or(0);
            Cell {
                column,
                row,
                columns: span(item.placement.column_span).min(columns - column),
                rows: span(item.placement.row_span).min(MAX_TRACKS - row),
            }
        })
        .collect();
    for (item, &cell) in items.iter().zip(&cells) {
        if item.placement.column.is_some() && item.placement.row.is_some() {
            occupied.insert(cell);
        }
    }
    let mut cursor = 0;
    let mut budget = MAX_TRACKS * MAX_TRACKS * 4;
    for (item, cell) in items.iter().zip(&mut cells) {
        let column = anchor(item.placement.column);
        let row = anchor(item.placement.row);
        if column.is_some() && row.is_some() {
            continue;
        }
        let start = row.map_or(if column.is_some() { 0 } else { cursor }, |r| r * columns);
        let end = row.map_or(columns * MAX_TRACKS, |r| (r + 1) * columns);
        for position in start..end {
            if budget == 0 {
                break;
            }
            budget -= 1;
            let c = position % columns;
            let r = position / columns;
            if column.is_some_and(|fixed| fixed != c)
                || c + cell.columns > columns
                || r + cell.rows > MAX_TRACKS
            {
                continue;
            }
            let candidate = Cell {
                column: c,
                row: r,
                ..*cell
            };
            if occupied.free(candidate) {
                *cell = candidate;
                if column.is_none() && row.is_none() {
                    cursor = position + cell.columns;
                }
                break;
            }
        }
        occupied.insert(*cell);
    }
    let rows = cells
        .iter()
        .map(|c| c.row + c.rows)
        .max()
        .unwrap_or(0)
        .max(stack.row_tracks.len().min(MAX_TRACKS));
    (cells, columns, rows)
}

/// Resolve authored grid anchors without measuring artwork.
/// Args: frame settings and child placements in document order.
/// Returns zero-based column, row, column span and row span for every input child.
pub(crate) fn grid_cells(
    stack: &AutoStack,
    placements: &[GridPlacement],
) -> Vec<(u32, u32, u32, u32)> {
    let items: Vec<_> = placements
        .iter()
        .map(|&placement| Item {
            size: Pt::ZERO,
            placement,
            fill_height: false,
        })
        .collect();
    place(stack, &items)
        .0
        .into_iter()
        .map(|cell| {
            (
                cell.column as u32,
                cell.row as u32,
                cell.columns as u32,
                cell.rows as u32,
            )
        })
        .collect()
}

fn limits(track: &GridTrack) -> (f32, f32) {
    let min = track.min.map_or(0.0, pixels);
    (min, track.max.map_or(MAX_SIZE, pixels).max(min))
}

fn weight(track: &GridTrack) -> Option<f32> {
    if let GridTrackSize::Fraction(value) = track.size {
        Some(if value.is_finite() && value > 0.0 {
            value.min(MAX_SIZE)
        } else {
            1.0
        })
    } else {
        None
    }
}

fn distribute(
    tracks: &[GridTrack],
    sizes: &mut [f32],
    indices: &[usize],
    total: f32,
    weighted: bool,
) {
    let mut active = indices.to_vec();
    let mut remaining = total;
    while !active.is_empty() {
        let weights: f64 = active
            .iter()
            .map(|&i| {
                if weighted {
                    weight(&tracks[i]).unwrap_or(1.0) as f64
                } else {
                    1.0
                }
            })
            .sum();
        let candidates: Vec<_> = active
            .iter()
            .map(|&i| {
                let share = (remaining.max(0.0) as f64
                    * if weighted {
                        weight(&tracks[i]).unwrap_or(1.0) as f64
                    } else {
                        1.0
                    }
                    / weights) as f32;
                let (min, max) = limits(&tracks[i]);
                (
                    i,
                    share,
                    share.clamp(min.max(if weighted { 0.0 } else { sizes[i] }), max),
                )
            })
            .collect();
        let violation = candidates.iter().map(|&(_, _, actual)| actual).sum::<f32>() - remaining;
        let frozen: Vec<_> = candidates
            .iter()
            .filter(|&&(_, share, actual)| {
                if violation > 0.001 {
                    actual > share + 0.001
                } else if violation < -0.001 {
                    actual < share - 0.001
                } else {
                    (actual - share).abs() > 0.001
                }
            })
            .map(|&(i, _, actual)| (i, actual))
            .collect();
        if frozen.is_empty() {
            for (i, _, actual) in candidates {
                sizes[i] = actual;
            }
            break;
        }
        for (i, actual) in frozen {
            sizes[i] = actual;
            remaining -= actual;
            active.retain(|&n| n != i);
        }
    }
}

fn resolve(
    tracks: &[GridTrack],
    content: &[(usize, usize, f32)],
    spacing: f32,
    available: Option<f32>,
) -> Vec<f32> {
    let mut sizes: Vec<_> = tracks
        .iter()
        .map(|track| {
            let (min, max) = limits(track);
            match track.size {
                GridTrackSize::Fixed(size) => pixels(size).clamp(min, max),
                _ => min,
            }
        })
        .collect();
    let mut content = content.to_vec();
    content.sort_by_key(|&(start, count, _)| (count, start));
    for &(start, count, natural) in &content {
        let end = start + count;
        let required = (pixels(natural) - spacing * count.saturating_sub(1) as f32).max(0.0);
        let current: f32 = sizes[start..end].iter().sum();
        if required <= current {
            continue;
        }
        let flexible: Vec<_> = (start..end)
            .filter(|&i| tracks[i].size == GridTrackSize::Hug)
            .collect();
        let fixed: f32 = (start..end)
            .filter(|i| !flexible.contains(i))
            .map(|i| sizes[i])
            .sum();
        distribute(tracks, &mut sizes, &flexible, required - fixed, false);
    }
    let fractions: Vec<_> = (0..tracks.len())
        .filter(|&i| weight(&tracks[i]).is_some())
        .collect();
    if let Some(available) = available {
        let fixed: f32 = (0..tracks.len())
            .filter(|i| !fractions.contains(i))
            .map(|i| sizes[i])
            .sum();
        let remaining = pixels(available) - fixed - spacing * tracks.len().saturating_sub(1) as f32;
        distribute(tracks, &mut sizes, &fractions, remaining, true);
    } else {
        let mut unit = fractions
            .iter()
            .map(|&i| sizes[i] as f64 / weight(&tracks[i]).unwrap() as f64)
            .fold(0.0f64, f64::max);
        for &(start, count, natural) in &content {
            let indices: Vec<_> = (start..start + count)
                .filter(|&i| weight(&tracks[i]).is_some())
                .collect();
            let fixed: f32 = (start..start + count)
                .filter(|i| !indices.contains(i))
                .map(|i| sizes[i])
                .sum();
            let required = pixels(natural) - fixed - spacing * count.saturating_sub(1) as f32;
            let mut candidate = sizes.clone();
            distribute(tracks, &mut candidate, &indices, required, true);
            unit = indices
                .iter()
                .map(|&i| candidate[i] as f64 / weight(&tracks[i]).unwrap() as f64)
                .fold(unit, f64::max);
        }
        for i in fractions {
            let (min, max) = limits(&tracks[i]);
            sizes[i] =
                (unit * weight(&tracks[i]).unwrap() as f64).clamp(min as f64, max as f64) as f32;
        }
    }
    sizes
}

/// Resolve grid cells and tracks.
/// Args: authored settings, measured children, and available inner width/height (None for Hug).
/// Returns bounded track lengths and one cell per child in its original order.
pub(super) fn resolve_grid(stack: &AutoStack, items: &[Item], available: [Option<f32>; 2]) -> Grid {
    let (cells, columns, rows) = place(stack, items);
    let column_tracks: Vec<_> = (0..columns)
        .map(|i| stack.column_tracks.get(i).cloned().unwrap_or_default())
        .collect();
    let row_tracks: Vec<_> = (0..rows)
        .map(|i| {
            stack
                .row_tracks
                .get(i)
                .cloned()
                .unwrap_or_else(|| GridTrack {
                    size: if items.iter().zip(&cells).any(|(item, cell)| {
                        item.fill_height && (cell.row..cell.row + cell.rows).contains(&i)
                    }) {
                        GridTrackSize::Fraction(1.0)
                    } else {
                        GridTrackSize::Hug
                    },
                    min: None,
                    max: None,
                })
        })
        .collect();
    let widths: Vec<_> = items
        .iter()
        .zip(&cells)
        .map(|(item, cell)| (cell.column, cell.columns, item.size.x))
        .collect();
    let heights: Vec<_> = items
        .iter()
        .zip(&cells)
        .map(|(item, cell)| (cell.row, cell.rows, item.size.y))
        .collect();
    Grid {
        cells,
        columns: resolve(&column_tracks, &widths, pixels(stack.gap), available[0]),
        rows: resolve(&row_tracks, &heights, pixels(stack.cross_gap), available[1]),
    }
}

/// Validate authored tracks and placement.
/// Args: an optional frame stack and the child's grid placement. Returns a readable rejection.
pub(super) fn validate(stack: Option<&AutoStack>, placement: GridPlacement) -> Result<(), String> {
    if placement.column.is_some_and(|v| v >= MAX_TRACKS as u32)
        || placement.row.is_some_and(|v| v >= MAX_TRACKS as u32)
        || !(1..=MAX_TRACKS as u32).contains(&placement.column_span)
        || !(1..=MAX_TRACKS as u32).contains(&placement.row_span)
        || placement
            .column
            .is_some_and(|v| v + placement.column_span > MAX_TRACKS as u32)
        || placement
            .row
            .is_some_and(|v| v + placement.row_span > MAX_TRACKS as u32)
    {
        return Err("Grid anchors must be 0–1023 and spans must fit within 1024 tracks".into());
    }
    let Some(stack) = stack else {
        return Ok(());
    };
    if stack.column_tracks.len() > MAX_TRACKS || stack.row_tracks.len() > MAX_TRACKS {
        return Err("A grid supports at most 1024 rows and columns".into());
    }
    for track in stack.column_tracks.iter().chain(&stack.row_tracks) {
        let valid = |value: f32| value.is_finite() && (0.0..=MAX_SIZE).contains(&value);
        if track.min.is_some_and(|v| !valid(v))
            || track.max.is_some_and(|v| !valid(v))
            || matches!((track.min, track.max), (Some(min), Some(max)) if max < min)
            || match track.size {
                GridTrackSize::Fixed(v) => !valid(v),
                GridTrackSize::Hug => false,
                GridTrackSize::Fraction(v) => !valid(v) || v == 0.0,
            }
        {
            return Err("Grid tracks need finite pixel limits and positive fraction weights; minimum cannot exceed maximum".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(size: GridTrackSize) -> GridTrack {
        GridTrack {
            size,
            min: None,
            max: None,
        }
    }
    fn close(actual: &[f32], expected: &[f32]) {
        assert_eq!(actual.len(), expected.len());
        for (actual, expected) in actual.iter().zip(expected) {
            assert!((actual - expected).abs() < 0.01, "{actual} != {expected}");
        }
    }
    fn item(column: Option<u32>, row: Option<u32>, columns: u32, rows: u32) -> Item {
        Item {
            size: Pt::new(20.0, 30.0),
            placement: GridPlacement {
                column,
                row,
                column_span: columns,
                row_span: rows,
            },
            fill_height: false,
        }
    }

    #[test]
    fn independent_six_hundred_pixel_grid_oracle() {
        let stack = AutoStack {
            gap: 10.0,
            cross_gap: 10.0,
            column_tracks: vec![
                track(GridTrackSize::Fixed(80.0)),
                track(GridTrackSize::Fraction(1.0)),
                track(GridTrackSize::Fraction(2.0)),
            ],
            row_tracks: vec![
                track(GridTrackSize::Fixed(40.0)),
                track(GridTrackSize::Fraction(1.0)),
            ],
            ..Default::default()
        };
        let items = [
            item(Some(0), Some(0), 1, 1),
            item(Some(1), Some(0), 2, 2),
            item(None, None, 1, 1),
        ];
        let grid = resolve_grid(&stack, &items, [Some(600.0), Some(250.0)]);
        close(&grid.columns, &[80.0, 500.0 / 3.0, 1000.0 / 3.0]);
        close(&grid.rows, &[40.0, 200.0]);
        assert_eq!(
            grid.cells[1],
            Cell {
                column: 1,
                row: 0,
                columns: 2,
                rows: 2
            }
        );
        assert_eq!(
            grid.cells[2],
            Cell {
                column: 0,
                row: 1,
                columns: 1,
                rows: 1
            }
        );
        assert!((grid.columns[1] + 10.0 + grid.columns[2] - 510.0).abs() < 0.01);
    }

    #[test]
    fn fixed_hug_fraction_limits_and_spanning_content_use_independent_lengths() {
        let mut tracks = vec![
            track(GridTrackSize::Fixed(80.0)),
            track(GridTrackSize::Hug),
            track(GridTrackSize::Fraction(1.0)),
            track(GridTrackSize::Fraction(2.0)),
        ];
        tracks[1].max = Some(120.0);
        tracks[2].max = Some(100.0);
        close(
            &resolve(&tracks, &[(1, 1, 180.0)], 10.0, Some(600.0)),
            &[80.0, 120.0, 100.0, 270.0],
        );
        tracks[2].max = None;
        tracks[2].min = Some(250.0);
        close(
            &resolve(&tracks, &[(1, 1, 100.0)], 10.0, Some(600.0)),
            &[80.0, 100.0, 250.0, 140.0],
        );
        let tracks = [
            track(GridTrackSize::Fixed(80.0)),
            track(GridTrackSize::Hug),
            track(GridTrackSize::Hug),
        ];
        close(
            &resolve(&tracks, &[(1, 2, 210.0)], 10.0, None),
            &[80.0, 100.0, 100.0],
        );
        let tracks = [
            track(GridTrackSize::Fraction(1.0)),
            track(GridTrackSize::Fraction(2.0)),
        ];
        close(
            &resolve(&tracks, &[(0, 2, 310.0)], 10.0, None),
            &[100.0, 200.0],
        );
        close(
            &resolve(&tracks, &[(0, 1, 80.0), (1, 1, 120.0)], 10.0, None),
            &[80.0, 160.0],
        );
    }

    #[test]
    fn auto_cells_reserve_later_explicit_children_and_partial_anchors() {
        let stack = AutoStack {
            columns: 3,
            ..Default::default()
        };
        let items = [
            item(None, None, 1, 1),
            item(Some(0), Some(0), 2, 1),
            item(Some(2), None, 1, 2),
            item(None, Some(2), 2, 1),
        ];
        let (cells, columns, rows) = place(&stack, &items);
        assert_eq!(columns, 3);
        assert_eq!(rows, 3);
        assert_eq!(
            cells[0],
            Cell {
                column: 2,
                row: 0,
                columns: 1,
                rows: 1
            }
        );
        assert_eq!(
            cells[2],
            Cell {
                column: 2,
                row: 1,
                columns: 1,
                rows: 2
            }
        );
        assert_eq!(
            cells[3],
            Cell {
                column: 0,
                row: 2,
                columns: 2,
                rows: 1
            }
        );
    }

    #[test]
    fn malformed_grid_data_is_rejected_and_resolution_stays_bounded() {
        let stack = AutoStack {
            columns: u32::MAX,
            gap: f32::NAN,
            cross_gap: f32::INFINITY,
            column_tracks: vec![GridTrack {
                size: GridTrackSize::Fraction(f32::NAN),
                min: Some(f32::INFINITY),
                max: Some(-1.0),
            }],
            row_tracks: vec![track(GridTrackSize::Fixed(f32::INFINITY))],
            ..Default::default()
        };
        let malformed = item(Some(u32::MAX), Some(u32::MAX), u32::MAX, 0);
        assert!(validate(Some(&stack), malformed.placement).is_err());
        let grid = resolve_grid(
            &stack,
            &[malformed, item(None, None, 1, 1)],
            [Some(f32::NAN), Some(f32::INFINITY)],
        );
        assert!(grid.columns.len() <= MAX_TRACKS && grid.rows.len() <= MAX_TRACKS);
        assert!(
            grid.columns
                .iter()
                .chain(&grid.rows)
                .all(|v| v.is_finite() && *v >= 0.0)
        );
        assert!(
            grid.cells
                .iter()
                .all(|c| c.column + c.columns <= MAX_TRACKS && c.row + c.rows <= MAX_TRACKS)
        );
        assert!(
            validate(
                None,
                GridPlacement {
                    column_span: 0,
                    ..Default::default()
                }
            )
            .is_err()
        );
    }

    #[test]
    fn old_stack_and_child_payloads_keep_uniform_grid_defaults() {
        let layout: super::super::FrameLayout = serde_json::from_str(r#"{"frame":true,"stack":{"direction":"Vertical","flow":"Grid","gap":10,"padding":[10,10,10,10],"align":"Start","columns":2}}"#).unwrap();
        assert!(layout.grid.is_auto());
        let stack = layout.stack.unwrap();
        assert!(stack.column_tracks.is_empty() && stack.row_tracks.is_empty());
        assert_eq!(stack.columns, 2);
        assert!(
            !serde_json::to_string(&super::super::FrameLayout::default())
                .unwrap()
                .contains("grid")
        );
        let span: GridPlacement = serde_json::from_str(r#"{"column":0,"row":1}"#).unwrap();
        assert_eq!((span.column_span, span.row_span), (1, 1));
    }
}

use super::*;
use crate::layout::{GridTrack, GridTrackSize, StackAlign, StackAxis, StackFlow, StackJustify};

fn fixture() -> (Studio, u64, u64, Shape) {
    let mut studio = Studio::new();
    studio.doc = Document::new("Mixed stack fields", 1000., 500., 96.);
    studio.doc.layers = vec![Layer::vector("Frames")];
    studio.active_layer = Some(0);
    studio.history = History::default();
    let mut first = layout::make_frame(Pt::ZERO, Pt::new(600., 200.));
    first.layout.stack = Some(AutoStack {
        flow: StackFlow::Grid,
        gap: 12.,
        padding: [2., 3., 4., 5.],
        column_tracks: vec![
            GridTrack {
                size: GridTrackSize::Fixed(80.),
                min: Some(20.),
                max: Some(100.),
            },
            GridTrack::default(),
        ],
        row_tracks: vec![GridTrack {
            size: GridTrackSize::Fixed(40.),
            ..Default::default()
        }],
        ..Default::default()
    });
    let mut second = layout::make_frame(Pt::new(700., 0.), Pt::new(240., 180.));
    second.layout.stack = Some(AutoStack {
        direction: StackAxis::Horizontal,
        flow: StackFlow::Wrap,
        gap: 24.,
        cross_gap: 33.,
        padding: [11., 13., 17., 19.],
        align: StackAlign::End,
        justify: StackJustify::SpaceAround,
        columns: 7,
        column_tracks: vec![
            GridTrack {
                size: GridTrackSize::Hug,
                min: Some(30.),
                max: Some(90.),
            },
            GridTrack {
                size: GridTrackSize::Fraction(3.),
                ..Default::default()
            },
            GridTrack {
                size: GridTrackSize::Fixed(50.),
                ..Default::default()
            },
        ],
        row_tracks: vec![GridTrack {
            size: GridTrackSize::Fraction(2.),
            min: Some(7.),
            max: Some(200.),
        }],
    });
    let unrelated = Shape::new(
        Geom::Rect {
            origin: Pt::new(960., 400.),
            size: Pt::new(25., 25.),
            radius: 0.,
        },
        Style::default(),
    );
    let (primary, secondary) = (first.id, second.id);
    studio.doc.layers[0]
        .kind
        .shapes_mut()
        .unwrap()
        .extend([first, second, unrelated.clone()]);
    studio.selection = vec![(0, primary), (0, secondary)];
    (studio, primary, secondary, unrelated)
}

fn properties(studio: &Studio, id: u64) -> FrameLayout {
    studio.doc.find_shape(0, id).unwrap().layout.clone()
}

#[test]
fn explicit_primary_equal_gap_preserves_mixed_stack_members_undo_redo_and_reopen() {
    let (mut studio, primary, secondary, unrelated) = fixture();
    let edited = properties(&studio, primary);
    let mut expected = properties(&studio, secondary).stack.unwrap();
    expected.gap = 12.;
    let before = crate::project::encode(&studio.doc).unwrap();
    let target = studio.layout_inspector_target().unwrap();
    assert!(studio.apply_layout_inspector_with_fields(&target, &edited, &[LayoutStackField::Gap]));
    assert_eq!(properties(&studio, primary), edited);
    assert_eq!(properties(&studio, secondary).stack, Some(expected.clone()));
    assert_eq!(studio.doc.find_shape(0, unrelated.id), Some(&unrelated));
    assert_eq!(studio.history.len(), 1);
    let after = crate::project::encode(&studio.doc).unwrap();
    let target = studio.layout_inspector_target().unwrap();
    assert!(studio.apply_layout_inspector_with_fields(&target, &edited, &[LayoutStackField::Gap]));
    assert_eq!(
        studio.history.len(),
        1,
        "already equal values do not add history"
    );
    studio.undo();
    assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
    studio.redo();
    assert_eq!(crate::project::encode(&studio.doc).unwrap(), after);
    let reopened = crate::project::decode(&after).unwrap();
    assert_eq!(
        reopened.find_shape(0, secondary).unwrap().layout.stack,
        Some(expected)
    );
    assert_eq!(reopened.find_shape(0, unrelated.id), Some(&unrelated));
}

#[test]
fn legacy_gap_and_explicit_primary_equal_padding_preserve_other_edges_and_tracks() {
    let (mut studio, primary, secondary, _) = fixture();
    let target = studio.layout_inspector_target().unwrap();
    let mut edited = properties(&studio, primary);
    edited.stack.as_mut().unwrap().gap = 18.;
    let mut expected = properties(&studio, secondary).stack.unwrap();
    expected.gap = 18.;
    assert!(studio.apply_layout_inspector(&target, &edited));
    assert_eq!(properties(&studio, secondary).stack, Some(expected.clone()));
    let edited = properties(&studio, primary);
    expected.padding[0] = 2.;
    let target = studio.layout_inspector_target().unwrap();
    assert!(studio.apply_layout_inspector_with_fields(
        &target,
        &edited,
        &[LayoutStackField::Padding(0)]
    ));
    assert_eq!(properties(&studio, secondary).stack, Some(expected));
}

#[test]
fn explicit_primary_equal_track_members_preserve_limits_other_tracks_and_stack_fields() {
    let (mut studio, primary, secondary, _) = fixture();
    let edited = properties(&studio, primary);
    let mut expected = properties(&studio, secondary).stack.unwrap();
    expected.column_tracks[0].size = GridTrackSize::Fixed(80.);
    expected.column_tracks[0].max = Some(100.);
    expected.row_tracks[0].size = GridTrackSize::Fixed(40.);
    let target = studio.layout_inspector_target().unwrap();
    assert!(studio.apply_layout_inspector_with_fields(
        &target,
        &edited,
        &[
            LayoutStackField::ColumnTrackSize(0),
            LayoutStackField::ColumnTrackMax(0),
            LayoutStackField::RowTrackSize(0),
        ]
    ));
    assert_eq!(properties(&studio, secondary).stack, Some(expected));
    assert_eq!(studio.history.len(), 1);
}

#[test]
fn unaccepted_locked_missing_or_invalid_stack_fields_do_not_commit() {
    for rejection in [
        "unaccepted",
        "locked",
        "missing-stack",
        "padding-edge",
        "track-index",
        "mixed-structure",
    ] {
        let (mut studio, primary, secondary, _) = fixture();
        if rejection == "missing-stack" {
            studio
                .doc
                .find_shape_mut(0, secondary)
                .unwrap()
                .layout
                .stack = None;
        }
        if rejection == "locked" {
            studio.doc.find_shape_mut(0, secondary).unwrap().locked = true;
        }
        let target = studio.layout_inspector_target().unwrap();
        let mut edited = properties(&studio, primary);
        edited.stack.as_mut().unwrap().gap = 99.;
        let fields = match rejection {
            "unaccepted" => vec![],
            "padding-edge" => vec![LayoutStackField::Padding(4)],
            "track-index" => vec![LayoutStackField::ColumnTrackSize(999)],
            "mixed-structure" => {
                edited
                    .stack
                    .as_mut()
                    .unwrap()
                    .column_tracks
                    .push(GridTrack::default());
                vec![LayoutStackField::ColumnTracks]
            }
            _ => vec![LayoutStackField::Gap],
        };
        let before = crate::project::encode(&studio.doc).unwrap();
        assert_eq!(
            studio.apply_layout_inspector_with_fields(&target, &edited, &fields),
            rejection == "unaccepted",
            "{rejection}"
        );
        assert_eq!(
            crate::project::encode(&studio.doc).unwrap(),
            before,
            "{rejection}"
        );
        assert_eq!(studio.history.len(), 0, "{rejection}");
    }
}

#[test]
fn explicit_enable_initializes_missing_stacks_without_replacing_existing_settings() {
    let (mut studio, primary, secondary, _) = fixture();
    let secondary_stack = properties(&studio, secondary).stack;
    studio.doc.find_shape_mut(0, primary).unwrap().layout.stack = None;
    let target = studio.layout_inspector_target().unwrap();
    let mut edited = properties(&studio, primary);
    edited.stack = Some(AutoStack::default());
    assert!(studio.apply_layout_inspector_with_fields(
        &target,
        &edited,
        &[LayoutStackField::Enabled]
    ));
    assert_eq!(properties(&studio, primary).stack, edited.stack);
    assert_eq!(properties(&studio, secondary).stack, secondary_stack);
    let target = studio.layout_inspector_target().unwrap();
    edited.stack = None;
    assert!(studio.apply_layout_inspector_with_fields(
        &target,
        &edited,
        &[LayoutStackField::Enabled]
    ));
    assert!(properties(&studio, primary).stack.is_none());
    assert!(properties(&studio, secondary).stack.is_none());
}

use crate::{motion::Motion, object::Object, Vim};
use collections::HashMap;
use editor::{display_map::ToDisplayPoint, Bias};
use gpui::WindowContext;
use language::SelectionGoal;

pub fn toggle_comments_motion(
    vim: &mut Vim,
    motion: Motion,
    times: Option<usize>,
    cx: &mut WindowContext,
) {
    vim.stop_recording();
    vim.update_active_editor(cx, |_, editor, cx| {
        let text_layout_details = editor.text_layout_details(cx);
        editor.transact(cx, |editor, cx| {
            let mut selection_starts: HashMap<_, _> = Default::default();
            editor.change_selections(None, cx, |s| {
                s.move_with(|map, selection| {
                    let anchor = map.display_point_to_anchor(selection.head(), Bias::Right);
                    selection_starts.insert(selection.id, anchor);
                    motion.expand_selection(map, selection, times, false, &text_layout_details);
                });
            });
            editor.toggle_comments(&Default::default(), cx);
            editor.change_selections(None, cx, |s| {
                s.move_with(|map, selection| {
                    let anchor = selection_starts.remove(&selection.id).unwrap();
                    selection.collapse_to(anchor.to_display_point(map), SelectionGoal::None);
                });
            });
        });
    });
}

pub fn toggle_comments_object(vim: &mut Vim, object: Object, around: bool, cx: &mut WindowContext) {
    vim.stop_recording();
    vim.update_active_editor(cx, |_, editor, cx| {
        editor.transact(cx, |editor, cx| {
            let mut original_positions: HashMap<_, _> = Default::default();
            editor.change_selections(None, cx, |s| {
                s.move_with(|map, selection| {
                    let anchor = map.display_point_to_anchor(selection.head(), Bias::Right);
                    original_positions.insert(selection.id, anchor);
                    object.expand_selection(map, selection, around);
                });
            });
            editor.toggle_comments(&Default::default(), cx);
            editor.change_selections(None, cx, |s| {
                s.move_with(|map, selection| {
                    let anchor = original_positions.remove(&selection.id).unwrap();
                    selection.collapse_to(anchor.to_display_point(map), SelectionGoal::None);
                });
            });
        });
    });
}

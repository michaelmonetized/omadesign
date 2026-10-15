/// Translate a native clipboard chord without dropping its originating key.
/// Args: event buffer, logical/physical keys, modifiers and clipboard reader.
/// Returns whether the chord was handled; paste normalizes line endings.
pub(crate) fn push_clipboard_command(
    events: &mut Vec<egui::Event>,
    key: egui::Key,
    physical_key: Option<egui::Key>,
    modifiers: egui::Modifiers,
    read_clipboard: impl FnOnce() -> Option<String>,
) -> bool {
    let cut = is_cut_command(modifiers, key);
    let copy = is_copy_command(modifiers, key);
    let paste = is_paste_command(modifiers, key);
    if !(cut || copy || paste) {
        return false;
    }
    events.push(egui::Event::Key {
        key,
        physical_key,
        pressed: true,
        repeat: false,
        modifiers,
    });
    if cut {
        events.push(egui::Event::Cut);
    } else if copy {
        events.push(egui::Event::Copy);
    } else if let Some(contents) = read_clipboard() {
        let contents = contents.replace("\r\n", "\n");
        if !contents.is_empty() {
            events.push(egui::Event::Paste(contents));
        }
    }
    true
}

fn is_cut_command(modifiers: egui::Modifiers, keycode: egui::Key) -> bool {
    keycode == egui::Key::Cut
        || ((modifiers.command || modifiers.ctrl || modifiers.mac_cmd) && keycode == egui::Key::X)
        || (cfg!(target_os = "windows") && modifiers.shift && keycode == egui::Key::Delete)
}

fn is_copy_command(modifiers: egui::Modifiers, keycode: egui::Key) -> bool {
    keycode == egui::Key::Copy
        || ((modifiers.command || modifiers.ctrl || modifiers.mac_cmd) && keycode == egui::Key::C)
        || (cfg!(target_os = "windows") && modifiers.ctrl && keycode == egui::Key::Insert)
}

fn is_paste_command(modifiers: egui::Modifiers, keycode: egui::Key) -> bool {
    keycode == egui::Key::Paste
        || ((modifiers.command || modifiers.ctrl || modifiers.mac_cmd) && keycode == egui::Key::V)
        // Linux clipboard-history pickers also inject Shift+Insert, including
        // image-only selections for which read_clipboard returns no text.
        || (modifiers.shift && keycode == egui::Key::Insert)
}

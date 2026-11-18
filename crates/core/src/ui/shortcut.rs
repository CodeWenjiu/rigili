use gpui_kit::component::dialog::Cancel;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;

const HELP_CONTEXT: &str = "ShortcutsHelp";

actions!(
    shortcut,
    [
        /// 键盘快捷键
        ShowShortcuts,
    ]
);

/// Registers the help dialog's key handling once per application.
pub fn init_shortcut_help(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("e", Cancel, Some("Dialog && !Input")),
        KeyBinding::new("enter", NoAction, Some(HELP_CONTEXT)),
        KeyBinding::new("escape", NoAction, Some(HELP_CONTEXT)),
    ]);
}

pub fn open_shortcut_help(
    window: &mut Window,
    cx: &mut App,
    title: &'static str,
    rows: Vec<(String, String)>,
) {
    let help = cx.new(|cx| ShortcutHelp::new(rows, cx));
    window.open_dialog(cx, move |dialog, _, _| {
        dialog.title(title).w(px(560.)).child(help.clone())
    });
}

/// Formats a keystroke description such as `ctrl-shift-[` for the shortcut
/// help UI.
pub fn display_keys(keys: &str) -> String {
    keys.split('-').map(label).collect::<Vec<_>>().join("+")
}

fn label(part: &str) -> String {
    match part {
        "ctrl" => "Ctrl".into(),
        "shift" => "Shift".into(),
        "alt" => "Alt".into(),
        "super" => "Super".into(),
        "fn" => "Fn".into(),
        "cmd" => "Cmd".into(),
        "left" => "←".into(),
        "right" => "→".into(),
        "up" => "↑".into(),
        "down" => "↓".into(),
        "enter" => "Enter".into(),
        "escape" => "Esc".into(),
        "space" => "Space".into(),
        "tab" => "Tab".into(),
        "backspace" => "Backspace".into(),
        "delete" => "Delete".into(),
        "home" => "Home".into(),
        "end" => "End".into(),
        "pageup" => "PageUp".into(),
        "pagedown" => "PageDown".into(),
        "{" => "Shift+[".into(),
        "}" => "Shift+]".into(),
        other => other.to_uppercase(),
    }
}

struct ShortcutHelp {
    focus: FocusHandle,
    focus_once: bool,
    rows: Vec<(String, String)>,
}

impl ShortcutHelp {
    fn new(rows: Vec<(String, String)>, cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
            focus_once: true,
            rows,
        }
    }
}

impl Render for ShortcutHelp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.focus_once {
            self.focus_once = false;
            let focus = self.focus.clone();
            window.defer(cx, move |window, cx| focus.focus(window, cx));
        }

        let muted = cx.theme().muted_foreground;
        let foreground = cx.theme().foreground;
        let border = cx.theme().border;
        let background = cx.theme().muted;

        v_flex()
            .id("shortcuts-help")
            .track_focus(&self.focus)
            .key_context(HELP_CONTEXT)
            .on_key_down(cx.listener(|_, event: &KeyDownEvent, window, cx| {
                let modifiers = event.keystroke.modifiers;
                let plain = !modifiers.control && !modifiers.alt && !modifiers.platform;
                let back = event.keystroke.key == "e"
                    || event.keystroke.key == "?"
                    || event.keystroke.key_char.as_deref() == Some("?");
                if plain && back {
                    cx.stop_propagation();
                    window.close_dialog(cx);
                }
            }))
            .w_full()
            .gap_1()
            .max_h(px(560.))
            .overflow_y_scroll()
            .children(self.rows.iter().map(|(keys, label)| {
                h_flex()
                    .w_full()
                    .h(px(30.))
                    .items_center()
                    .justify_between()
                    .gap_4()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_sm()
                            .child(label.clone()),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .px_2()
                            .py_1()
                            .rounded(px(4.))
                            .border_1()
                            .border_color(border)
                            .bg(background)
                            .text_color(foreground)
                            .text_xs()
                            .child(keys.clone()),
                    )
            }))
            .child(
                div()
                    .pt_3()
                    .text_xs()
                    .text_color(muted)
                    .child("按 E 返回；输入框内仍用 Esc 取消输入"),
            )
    }
}

#[cfg(test)]
mod tests {
    use gpui_kit::component::dialog::Cancel;
    use gpui_kit::{KeyBinding, KeyContext, Keymap, Keystroke, NoAction};

    use super::{HELP_CONTEXT, display_keys};

    #[test]
    fn formats_shortcut_keys_for_display() {
        assert_eq!(display_keys("r"), "R");
        assert_eq!(display_keys("/"), "/");
        assert_eq!(display_keys("?"), "?");
        assert_eq!(display_keys("ctrl-1"), "Ctrl+1");
        assert_eq!(display_keys("ctrl-shift-tab"), "Ctrl+Shift+Tab");
        assert_eq!(display_keys("shift-["), "Shift+[");
        assert_eq!(display_keys("shift-left"), "Shift+←");
        assert_eq!(display_keys("space"), "Space");
    }

    #[test]
    fn help_swallows_dialog_enter_and_escape_but_back_closes() {
        let keymap = Keymap::new(vec![
            KeyBinding::new("escape", Cancel, Some("Dialog")),
            KeyBinding::new("e", Cancel, Some("Dialog && !Input")),
            KeyBinding::new("enter", NoAction, Some(HELP_CONTEXT)),
            KeyBinding::new("escape", NoAction, Some(HELP_CONTEXT)),
        ]);
        let stack = vec![
            KeyContext::parse("Dialog").expect("dialog context"),
            KeyContext::parse(HELP_CONTEXT).expect("help context"),
        ];

        for key in ["enter", "escape"] {
            let (bindings, pending) =
                keymap.bindings_for_input(&[Keystroke::parse(key).expect("keystroke")], &stack);
            assert!(
                bindings.is_empty(),
                "{key} must be inert in the help surface"
            );
            assert!(!pending);
        }

        let (bindings, _) =
            keymap.bindings_for_input(&[Keystroke::parse("e").expect("keystroke")], &stack);
        assert_eq!(bindings.len(), 1);
        assert!(bindings[0].action().partial_eq(&Cancel));
    }
}

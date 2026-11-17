use std::rc::Rc;

use gpui_kit::*;
use rigili_core::ShowShortcuts;

pub(crate) const MAIN: &str = "Main";

actions!(
    shell,
    [
        /// 刷新当前页
        ReloadPage,
        /// 搜索
        FocusSearch,
        /// 切换到播放器
        FocusPlayer,
        /// 返回上一级
        Back,
    ]
);

struct Command {
    keys: &'static str,
    context: &'static str,
    action: fn() -> Box<dyn Action>,
}

const COMMANDS: &[Command] = &[
    Command {
        keys: "/",
        context: MAIN,
        action: || Box::new(FocusSearch),
    },
    Command {
        keys: "e",
        context: MAIN,
        action: || Box::new(Back),
    },
    Command {
        keys: "r",
        context: MAIN,
        action: || Box::new(ReloadPage),
    },
    Command {
        keys: "s",
        context: MAIN,
        action: || Box::new(FocusPlayer),
    },
    Command {
        keys: "?",
        context: MAIN,
        action: || Box::new(ShowShortcuts),
    },
];

pub(crate) fn rows(cx: &App) -> Vec<(String, String)> {
    let documents = cx.action_documentation();
    COMMANDS
        .iter()
        .map(|command| {
            let action = (command.action)();
            let label = documents
                .get(action.name())
                .copied()
                .unwrap_or_else(|| action.name());
            (rigili_core::display_keys(command.keys), label.to_owned())
        })
        .collect()
}

pub(crate) fn open_help(window: &mut Window, cx: &mut App) {
    let shortcuts = rows(cx);
    rigili_core::open_shortcut_help(window, cx, "主界面快捷键", shortcuts);
}

fn binding_predicate(context: &str) -> KeyBindingContextPredicate {
    let source = format!("{context} && !{}", rigili_core::TEXT_INPUT_CONTEXT);
    KeyBindingContextPredicate::parse(&source).expect("static key context")
}

pub(crate) fn bind(cx: &mut App) {
    let bindings = COMMANDS
        .iter()
        .map(|command| {
            KeyBinding::load(
                command.keys,
                (command.action)(),
                Some(Rc::new(binding_predicate(command.context))),
                false,
                None,
                cx,
            )
            .expect("static keystroke")
        })
        .collect::<Vec<_>>();
    cx.bind_keys(bindings);
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use gpui_kit::{KeyBinding, KeyBindingContextPredicate, KeyContext, Keystroke, Modifiers};
    use rigili_core::ShowShortcuts;

    use super::{COMMANDS, MAIN, binding_predicate};

    #[test]
    fn commands_parse_and_do_not_conflict() {
        let mut seen = BTreeSet::new();

        for command in COMMANDS {
            assert!(
                Keystroke::parse(command.keys).is_ok(),
                "invalid keystroke: {}",
                command.keys
            );
            assert!(
                KeyBindingContextPredicate::parse(command.context).is_ok(),
                "invalid context: {}",
                command.context
            );
            assert!(
                seen.insert((command.keys, command.context)),
                "duplicate binding: {} in {}",
                command.keys,
                command.context
            );
            assert_eq!(command.context, MAIN);
        }

        let keys = COMMANDS
            .iter()
            .map(|command| command.keys)
            .collect::<BTreeSet<_>>();
        assert_eq!(keys, BTreeSet::from(["r", "s", "e", "?", "/"]));
    }

    #[test]
    fn question_mark_binding_accepts_shift_slash() {
        let binding = KeyBinding::new("?", ShowShortcuts, Some(MAIN));
        let typed = Keystroke {
            key: "/".into(),
            key_char: Some("?".into()),
            modifiers: Modifiers {
                shift: true,
                ..Default::default()
            },
        };

        assert!(binding.match_keystrokes(&[typed]).is_some());
    }

    #[gpui_kit::test]
    fn every_command_has_a_documented_action(cx: &mut gpui_kit::TestAppContext) {
        let documents = cx.update(|cx| cx.action_documentation().clone());
        for command in COMMANDS {
            let action = (command.action)();
            assert!(
                documents.contains_key(action.name()),
                "missing documentation for {}",
                action.name()
            );
        }

        let rows = cx.update(|cx| super::rows(cx));
        assert_eq!(rows.len(), COMMANDS.len());
        assert!(
            rows.iter()
                .all(|(keys, label)| !keys.is_empty() && !label.is_empty())
        );
    }

    #[test]
    fn stands_down_while_a_text_input_is_focused() {
        let predicate = binding_predicate(MAIN);
        let main = KeyContext::parse(MAIN).expect("main context");
        let input = KeyContext::parse(rigili_core::TEXT_INPUT_CONTEXT).expect("text input context");

        assert!(predicate.depth_of(std::slice::from_ref(&main)).is_some());
        assert!(predicate.depth_of(&[main, input]).is_none());
    }
}

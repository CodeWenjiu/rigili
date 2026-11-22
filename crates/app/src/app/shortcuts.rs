use std::rc::Rc;

use gpui_kit::*;
use rigili_core::ShowShortcuts;

pub(crate) const MAIN: &str = "Main";
pub(crate) const CARD: &str = "CardFocus";

actions!(
    shell,
    [
        /// 刷新
        ReloadPage,
        /// 搜索
        FocusSearch,
        /// 切换到播放器
        FocusPlayer,
        /// 返回上一级
        Back,
        /// 首页
        ShowHome,
        /// 我的
        ShowMine,
        /// 个人空间
        ToggleMySpace,
        /// 消息
        ToggleMessages,
        /// 上一个标签
        PreviousContentTab,
        /// 下一个标签
        NextContentTab,
        /// 上一个子标签
        PreviousSubTab,
        /// 下一个子标签
        NextSubTab,
        /// 键盘浏览卡片
        ToggleCardFocus,
        /// 选择左侧卡片
        SelectCardLeft,
        /// 选择右侧卡片
        SelectCardRight,
        /// 选择上方卡片
        SelectCardUp,
        /// 选择下方卡片
        SelectCardDown,
        /// 打开选中项
        OpenSelectedCard,
        /// 在新标签页打开选中项
        OpenSelectedCardInNewTab,
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
        keys: "1",
        context: MAIN,
        action: || Box::new(ShowHome),
    },
    Command {
        keys: "2",
        context: MAIN,
        action: || Box::new(ShowMine),
    },
    Command {
        // Shift+1/2 arrive as the symbols themselves: the platform folds the
        // shift into the keysym and drops the modifier.
        keys: "!",
        context: MAIN,
        action: || Box::new(ToggleMySpace),
    },
    Command {
        keys: "@",
        context: MAIN,
        action: || Box::new(ToggleMessages),
    },
    Command {
        keys: "[",
        context: MAIN,
        action: || Box::new(PreviousContentTab),
    },
    Command {
        keys: "]",
        context: MAIN,
        action: || Box::new(NextContentTab),
    },
    Command {
        // Shift+[ / Shift+] arrive as the symbols themselves.
        keys: "{",
        context: MAIN,
        action: || Box::new(PreviousSubTab),
    },
    Command {
        keys: "}",
        context: MAIN,
        action: || Box::new(NextSubTab),
    },
    Command {
        keys: "i",
        context: MAIN,
        action: || Box::new(ToggleCardFocus),
    },
    Command {
        keys: "h",
        context: CARD,
        action: || Box::new(SelectCardLeft),
    },
    Command {
        keys: "j",
        context: CARD,
        action: || Box::new(SelectCardDown),
    },
    Command {
        keys: "k",
        context: CARD,
        action: || Box::new(SelectCardUp),
    },
    Command {
        keys: "l",
        context: CARD,
        action: || Box::new(SelectCardRight),
    },
    Command {
        keys: "enter",
        context: CARD,
        action: || Box::new(OpenSelectedCard),
    },
    Command {
        keys: "ctrl-enter",
        context: CARD,
        action: || Box::new(OpenSelectedCardInNewTab),
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

    use super::{CARD, COMMANDS, MAIN, binding_predicate};

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
            assert!(command.context == MAIN || command.context == CARD);
        }

        let keys = COMMANDS
            .iter()
            .map(|command| command.keys)
            .collect::<BTreeSet<_>>();
        assert_eq!(
            keys,
            BTreeSet::from([
                "1", "2", "r", "s", "e", "!", "@", "[", "]", "{", "}", "?", "/", "i", "h", "j", "k", "l",
                "enter", "ctrl-enter"
            ])
        );
    }

    #[gpui_kit::test]
    fn ctrl_enter_matches_the_platform_keystroke(cx: &mut gpui_kit::TestAppContext) {
        cx.update(gpui_kit::init);
        let command = COMMANDS
            .iter()
            .find(|command| command.keys == "ctrl-enter")
            .expect("ctrl-enter binding");
        let binding = cx
            .update(|cx| {
                KeyBinding::load(command.keys, (command.action)(), None, false, None, cx)
            })
            .expect("static keystroke");
        let typed = Keystroke {
            key: "enter".into(),
            key_char: None,
            modifiers: Modifiers {
                control: true,
                ..Default::default()
            },
        };

        assert_eq!(binding.match_keystrokes(&[typed]), Some(false));
    }

    #[test]
    fn card_commands_only_apply_while_the_card_region_is_focused() {
        let main = KeyContext::parse(MAIN).expect("main context");
        let card = KeyContext::parse(CARD).expect("card context");
        let card_commands = COMMANDS
            .iter()
            .filter(|command| command.context == CARD)
            .collect::<Vec<_>>();
        assert_eq!(card_commands.len(), 6);

        for command in card_commands {
            let predicate = binding_predicate(command.context);
            assert!(
                predicate.depth_of(std::slice::from_ref(&main)).is_none(),
                "{} must not reach the main context",
                command.keys
            );
            assert!(
                predicate
                    .depth_of(&[main.clone(), card.clone()])
                    .is_some(),
                "{} must reach the card context",
                command.keys
            );
        }

        let enter = COMMANDS
            .iter()
            .find(|command| command.keys == "enter")
            .expect("enter binding");
        assert_eq!(enter.context, CARD);
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
    fn shifted_symbols_match_what_the_platform_reports(cx: &mut gpui_kit::TestAppContext) {
        cx.update(gpui_kit::init);
        let shifted = COMMANDS
            .iter()
            .filter(|command| matches!(command.keys, "!" | "@" | "{" | "}"))
            .collect::<Vec<_>>();
        assert_eq!(shifted.len(), 4);

        for command in shifted {
            let typed = Keystroke {
                key: command.keys.to_owned(),
                key_char: Some(command.keys.to_owned()),
                modifiers: Modifiers::none(),
            };
            let binding = cx
                .update(|cx| {
                    KeyBinding::load(command.keys, (command.action)(), None, false, None, cx)
                })
                .expect("static keystroke");
            assert_eq!(
                binding.match_keystrokes(&[typed]),
                Some(false),
                "{} does not match the keystroke the platform reports",
                command.keys
            );
        }
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

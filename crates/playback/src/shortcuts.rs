use std::rc::Rc;

use gpui_kit::*;

pub(crate) const PLAYER: &str = "Player";

actions!(
    player,
    [
        /// 播放 / 暂停
        TogglePlayback,
        /// 后退 5 秒
        SeekBack,
        /// 后退历史
        HistoryBack,
        /// 前进历史
        HistoryForward,
        /// 减速
        SpeedDown,
        /// 加速
        SpeedUp,
        /// 上一个分P
        PreviousPage,
        /// 下一个分P
        NextPage,
        /// 静音
        ToggleMute,
        /// 全屏
        ToggleFullscreen,
        /// 视频信息
        ToggleInfo,
        /// 评论
        ToggleComments,
        /// 弹幕开关
        ToggleDanmaku,
        /// 重新加载
        Reload,
        /// 回到主窗口
        SwitchToMain,
    ]
);

struct Command {
    keys: &'static str,
    context: &'static str,
    action: fn() -> Box<dyn Action>,
}

const COMMANDS: &[Command] = &[
    Command {
        keys: "space",
        context: PLAYER,
        action: || Box::new(TogglePlayback),
    },
    Command {
        keys: "left",
        context: PLAYER,
        action: || Box::new(SeekBack),
    },
    Command {
        keys: "shift-left",
        context: PLAYER,
        action: || Box::new(SpeedDown),
    },
    Command {
        keys: "shift-right",
        context: PLAYER,
        action: || Box::new(SpeedUp),
    },
    Command {
        keys: "shift-[",
        context: PLAYER,
        action: || Box::new(PreviousPage),
    },
    Command {
        keys: "shift-]",
        context: PLAYER,
        action: || Box::new(NextPage),
    },
    Command {
        keys: "<",
        context: PLAYER,
        action: || Box::new(HistoryBack),
    },
    Command {
        keys: ">",
        context: PLAYER,
        action: || Box::new(HistoryForward),
    },
    Command {
        keys: "m",
        context: PLAYER,
        action: || Box::new(ToggleMute),
    },
    Command {
        keys: "f",
        context: PLAYER,
        action: || Box::new(ToggleFullscreen),
    },
    Command {
        keys: "x",
        context: PLAYER,
        action: || Box::new(ToggleInfo),
    },
    Command {
        keys: "c",
        context: PLAYER,
        action: || Box::new(ToggleComments),
    },
    Command {
        keys: "d",
        context: PLAYER,
        action: || Box::new(ToggleDanmaku),
    },
    Command {
        keys: "r",
        context: PLAYER,
        action: || Box::new(Reload),
    },
    Command {
        keys: "s",
        context: PLAYER,
        action: || Box::new(SwitchToMain),
    },
    Command {
        keys: "?",
        context: PLAYER,
        action: || Box::new(rigili_core::ShowShortcuts),
    },
];

fn binding_predicate(context: &str) -> KeyBindingContextPredicate {
    let source = format!("{context} && !{}", rigili_core::TEXT_INPUT_CONTEXT);
    KeyBindingContextPredicate::parse(&source).expect("static key context")
}

pub fn bind(cx: &mut App) {
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

pub fn rows(cx: &App) -> Vec<(String, String)> {
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use gpui_kit::{KeyBindingContextPredicate, KeyContext, Keystroke};

    use super::{COMMANDS, PLAYER, binding_predicate};

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
        }

        assert!(COMMANDS.iter().all(|command| command.context == PLAYER));
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
        let predicate = binding_predicate(PLAYER);
        let player = KeyContext::parse(PLAYER).expect("player context");
        let input = KeyContext::parse(rigili_core::TEXT_INPUT_CONTEXT).expect("text input context");

        assert!(predicate.depth_of(std::slice::from_ref(&player)).is_some());
        assert!(predicate.depth_of(&[player, input]).is_none());
    }
}

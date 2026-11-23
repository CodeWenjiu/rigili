use std::rc::Rc;

use gpui_kit::*;

pub(crate) const PLAYER: &str = "Player";
pub(crate) const RELATED: &str = "RelatedVideos";
pub(crate) const COMMENTS: &str = "CommentList";

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
        /// 写评论
        WriteComment,
        /// 弹幕开关
        ToggleDanmaku,
        /// 字幕开关
        ToggleSubtitles,
        /// 重新加载
        Reload,
        /// 回到主窗口
        SwitchToMain,
        /// 键盘浏览信息栏
        TogglePanelNavigation,
        /// 上一个相关推荐
        PreviousRelated,
        /// 下一个相关推荐
        NextRelated,
        /// 播放相关推荐
        PlayRelated,
        /// 在新标签页播放相关推荐
        PlayRelatedInNewTab,
        /// 上一条评论
        PreviousComment,
        /// 下一条评论
        NextComment,
        /// 展开回复
        ExpandReplies,
        /// 打开评论图片
        OpenCommentImage,
        /// 返回评论列表
        BackComments,
        /// 关闭当前标签页
        CloseTab,
        /// 下一个标签页
        NextTab,
        /// 上一个标签页
        PreviousTab,
        /// 返回上一级
        CloseCommentThread,
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
        // Shift+[ arrives as the symbol itself: the platform folds the shift
        // into the keysym and drops the modifier, so `shift-[` never matches.
        keys: "{",
        context: PLAYER,
        action: || Box::new(PreviousPage),
    },
    Command {
        keys: "}",
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
        keys: "/",
        context: PLAYER,
        action: || Box::new(WriteComment),
    },
    Command {
        keys: "d",
        context: PLAYER,
        action: || Box::new(ToggleDanmaku),
    },
    Command {
        keys: "v",
        context: PLAYER,
        action: || Box::new(ToggleSubtitles),
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
        keys: "i",
        context: PLAYER,
        action: || Box::new(TogglePanelNavigation),
    },
    Command {
        keys: "k",
        context: RELATED,
        action: || Box::new(PreviousRelated),
    },
    Command {
        keys: "j",
        context: RELATED,
        action: || Box::new(NextRelated),
    },
    Command {
        keys: "enter",
        context: RELATED,
        action: || Box::new(PlayRelated),
    },
    Command {
        keys: "ctrl-enter",
        context: RELATED,
        action: || Box::new(PlayRelatedInNewTab),
    },
    Command {
        keys: "k",
        context: COMMENTS,
        action: || Box::new(PreviousComment),
    },
    Command {
        keys: "j",
        context: COMMENTS,
        action: || Box::new(NextComment),
    },
    Command {
        keys: "l",
        context: COMMENTS,
        action: || Box::new(ExpandReplies),
    },
    Command {
        keys: "enter",
        context: COMMENTS,
        action: || Box::new(OpenCommentImage),
    },
    Command {
        keys: "h",
        context: COMMENTS,
        action: || Box::new(BackComments),
    },
    Command {
        keys: "ctrl-w",
        context: PLAYER,
        action: || Box::new(CloseTab),
    },
    Command {
        keys: "tab",
        context: PLAYER,
        action: || Box::new(NextTab),
    },
    Command {
        keys: "shift-tab",
        context: PLAYER,
        action: || Box::new(PreviousTab),
    },
    Command {
        keys: "?",
        context: PLAYER,
        action: || Box::new(rigili_core::ShowShortcuts),
    },
    Command {
        keys: "e",
        context: PLAYER,
        action: || Box::new(CloseCommentThread),
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

    use gpui_kit::{KeyBinding, KeyBindingContextPredicate, KeyContext, Keystroke, Modifiers};

    use super::{COMMANDS, COMMENTS, PLAYER, RELATED, binding_predicate};

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

        assert!(COMMANDS.iter().all(|command| {
            command.context == PLAYER || command.context == RELATED || command.context == COMMENTS
        }));
    }

    #[test]
    fn comment_commands_only_apply_while_the_comment_list_is_focused() {
        let player = KeyContext::parse(PLAYER).expect("player context");
        let comments = KeyContext::parse(COMMENTS).expect("comments context");
        let comment_commands = COMMANDS
            .iter()
            .filter(|command| command.context == COMMENTS)
            .collect::<Vec<_>>();
        assert_eq!(comment_commands.len(), 5);

        for command in comment_commands {
            let predicate = binding_predicate(command.context);
            assert!(
                predicate.depth_of(std::slice::from_ref(&player)).is_none(),
                "{} must not reach the player context",
                command.keys
            );
            assert!(
                predicate
                    .depth_of(&[player.clone(), comments.clone()])
                    .is_some(),
                "{} must reach the comments context",
                command.keys
            );
        }
    }

    #[test]
    fn related_commands_only_apply_while_the_related_list_is_focused() {
        let player = KeyContext::parse(PLAYER).expect("player context");
        let related = KeyContext::parse(RELATED).expect("related context");
        let related_commands = COMMANDS
            .iter()
            .filter(|command| command.context == RELATED)
            .collect::<Vec<_>>();
        assert_eq!(related_commands.len(), 4);

        for command in related_commands {
            let predicate = binding_predicate(command.context);
            assert!(
                predicate.depth_of(std::slice::from_ref(&player)).is_none(),
                "{} must not reach the player context",
                command.keys
            );
            assert!(
                predicate
                    .depth_of(&[player.clone(), related.clone()])
                    .is_some(),
                "{} must reach the related context",
                command.keys
            );
        }

        let enter = COMMANDS
            .iter()
            .find(|command| command.keys == "enter")
            .expect("enter binding");
        assert_eq!(enter.context, RELATED);
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

    #[gpui_kit::test]
    fn page_shortcuts_match_the_shifted_symbol(cx: &mut gpui_kit::TestAppContext) {
        cx.update(gpui_kit::init);
        let page_commands = COMMANDS
            .iter()
            .filter(|command| matches!(command.keys, "{" | "}"))
            .collect::<Vec<_>>();
        assert_eq!(page_commands.len(), 2);

        for command in page_commands {
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

    #[test]
    fn stands_down_while_a_text_input_is_focused() {
        let predicate = binding_predicate(PLAYER);
        let player = KeyContext::parse(PLAYER).expect("player context");
        let input = KeyContext::parse(rigili_core::TEXT_INPUT_CONTEXT).expect("text input context");

        assert!(predicate.depth_of(std::slice::from_ref(&player)).is_some());
        assert!(predicate.depth_of(&[player, input]).is_none());
    }
}

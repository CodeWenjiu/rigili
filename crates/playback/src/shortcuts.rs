use std::rc::Rc;

use gpui_kit::*;

pub(crate) const PLAYER: &str = "Player";

actions!(
    player,
    [
        TogglePlayback,
        SeekBack,
        HistoryBack,
        HistoryForward,
        SpeedDown,
        SpeedUp,
        ToggleMute,
        ToggleFullscreen,
        ToggleInfo,
        ToggleComments,
        ToggleDanmaku,
        Reload,
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

    #[test]
    fn stands_down_while_a_text_input_is_focused() {
        let predicate = binding_predicate(PLAYER);
        let player = KeyContext::parse(PLAYER).expect("player context");
        let input = KeyContext::parse(rigili_core::TEXT_INPUT_CONTEXT).expect("text input context");

        assert!(predicate.depth_of(std::slice::from_ref(&player)).is_some());
        assert!(predicate.depth_of(&[player, input]).is_none());
    }
}

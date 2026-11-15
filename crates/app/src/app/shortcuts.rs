use std::rc::Rc;

use gpui_kit::*;

pub(crate) const MAIN: &str = "Main";

actions!(shell, [ReloadPage, FocusSearch, FocusPlayer]);

struct Command {
    keys: &'static str,
    context: &'static str,
    action: fn() -> Box<dyn Action>,
}

const COMMANDS: &[Command] = &[
    Command {
        keys: "r",
        context: MAIN,
        action: || Box::new(ReloadPage),
    },
    Command {
        keys: "i",
        context: MAIN,
        action: || Box::new(FocusSearch),
    },
    Command {
        keys: "s",
        context: MAIN,
        action: || Box::new(FocusPlayer),
    },
];

pub(crate) fn bind(cx: &mut App) {
    let bindings = COMMANDS
        .iter()
        .map(|command| {
            let predicate =
                KeyBindingContextPredicate::parse(command.context).expect("static key context");
            KeyBinding::load(
                command.keys,
                (command.action)(),
                Some(Rc::new(predicate)),
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

    use gpui_kit::{KeyBindingContextPredicate, Keystroke};

    use super::{COMMANDS, MAIN};

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
        assert_eq!(keys, BTreeSet::from(["r", "i", "s"]));
    }
}

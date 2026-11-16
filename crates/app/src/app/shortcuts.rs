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

    use gpui_kit::{KeyBindingContextPredicate, KeyContext, Keystroke};

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
        assert_eq!(keys, BTreeSet::from(["r", "i", "s"]));
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

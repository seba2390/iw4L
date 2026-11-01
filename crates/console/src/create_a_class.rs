use bevy::prelude::*;

use crate::{ConsoleCommand, ConsoleLine, ConsoleRegistry, ConsoleSettings, ConsoleState};

/// Black Ops' create-a-class commands, which its menus run with `execnow`:
/// each edits a custom class in the local profile's stats.
const COMMANDS: [(&str, Option<&str>, &str); 4] = [
    (
        "equipclass",
        Some("equipClass"),
        "equipClass <customclassname> <item> — put an item (by name) in a custom class",
    ),
    (
        "equipclassitem",
        Some("equipClassItem"),
        "equipClassItem <customclassname> <itemindex> — put an item (by its mp/statsTable.csv number) in a custom class",
    ),
    (
        "equipclassattachment",
        Some("equipClassAttachment"),
        "equipClassAttachment <customclassname> <item> <attachment> [point] — put a weapon and an attachment on it in a custom class",
    ),
    (
        "copyclass",
        None,
        "copyclass <classIndexToCopy> <classIndexToReplace> — copy a custom class over another",
    ),
];

pub(crate) fn register(registry: &mut ConsoleRegistry) {
    for (name, alias, usage) in COMMANDS {
        if registry.resolve(name).is_none() {
            let spec = crate::CommandSpec::new(name).usage(usage);
            registry.register(match alias {
                Some(alias) => spec.alias(alias),
                None => spec,
            });
        }
    }
}

pub(crate) fn route(
    mut events: MessageReader<ConsoleCommand>,
    mut console: ResMut<ConsoleState>,
    settings: Res<ConsoleSettings>,
    mut line: ResMut<ConsoleLine>,
    authority: Option<ResMut<net::AuthorityWorld>>,
) {
    let mut authority = authority;
    for cmd in events.read() {
        let name = cmd.name.to_ascii_lowercase();
        if !COMMANDS.iter().any(|(command, ..)| *command == name) {
            continue;
        }
        let result = match authority.as_deref_mut() {
            Some(authority) => authority.0.class_command(&name, &cmd.args),
            None => Some(Err("not in a match".into())),
        };
        let messages = match result {
            Some(Ok(messages)) => messages,
            Some(Err(error)) => vec![format!("{name}: {error}")],
            None => continue,
        };
        for message in messages {
            line.0 = message.clone();
            console.echo(message, settings.log_capacity);
        }
    }
}

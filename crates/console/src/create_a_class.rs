use bevy::prelude::*;

use crate::{ConsoleCommand, ConsoleLine, ConsoleRegistry, ConsoleSettings, ConsoleState};

/// Black Ops' create-a-class commands, which its menus run with `execnow`:
/// each edits a custom class in the local profile's stats.
const COMMANDS: [(&str, Option<&str>, &str); 12] = [
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
    (
        "equipdefaultclass",
        Some("equipDefaultClass"),
        "equipDefaultClass <customclassname> <defaultClass> — fill a custom class from a default class (CLASS_SMG, …)",
    ),
    (
        "equipdefaultitemtoslot",
        Some("equipDefaultItemToSlot"),
        "equipDefaultItemToSlot <customclassname> <slotname> — empty one slot of a custom class",
    ),
    (
        "equipdefaultglobalitemtoslot",
        Some("equipDefaultGlobalItemToSlot"),
        "equipDefaultGlobalItemToSlot <slotname> — empty a killstreak or deathstreak slot",
    ),
    (
        "equipdefaultglobalitems",
        Some("equipDefaultGlobalItems"),
        "equipDefaultGlobalItems — the default killstreaks and deathstreak",
    ),
    (
        "clearitemnew",
        Some("clearItemNew"),
        "clearItemNew <itemname> — an item is no longer marked new",
    ),
    (
        "clearallitemsnew",
        Some("clearAllItemsNew"),
        "clearAllItemsNew — no item is marked new",
    ),
    (
        "clearallattachmentsforitem",
        Some("clearAllAttachmentsForItem"),
        "clearAllAttachmentsForItem <customClassName> <itemIndex> — take every attachment off a custom class's weapon",
    ),
    (
        "clearattachmentforitematpoint",
        Some("clearAttachmentForItemAtPoint"),
        "clearAttachmentForItemAtPoint <customClassName> <itemIndex> <attachmentPoint> — take an attachment off at a point (1 top, 2 bottom, 3 trigger, 4 muzzle)",
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

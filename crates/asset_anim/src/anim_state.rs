use xmodel_runtime::{AnimStateClip, AnimStateTable};

pub fn parse_anim_states(source: &[u8]) -> Result<AnimStateTable, &'static str> {
    let text = std::str::from_utf8(source).map_err(|_| "animation states are not UTF-8")?;
    let mut table = AnimStateTable::default();
    let mut pending: Option<(String, Vec<String>)> = None;
    let mut active: Option<(String, bool, Vec<AnimStateClip>)> = None;
    for line in text.lines() {
        let line = line
            .split("//")
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        let line = line.as_str();
        if line.is_empty() {
            continue;
        }
        if line == "{" {
            let (name, properties) = pending.take().ok_or("animation state has no header")?;
            if active.is_some() {
                return Err("nested animation state");
            }
            let aliased = properties.iter().any(|p| p == "aliased");
            table.properties.insert(name.clone(), properties);
            active = Some((name, aliased, Vec::new()));
        } else if line == "}" {
            let (name, _, entries) = active.take().ok_or("animation state has no body")?;
            if table.states.insert(name, entries).is_some() {
                return Err("duplicate animation state");
            }
        } else if let Some((_, aliased, entries)) = &mut active {
            let words: Vec<_> = line.split_whitespace().collect();
            let (alias, clip) = match (aliased, words.as_slice()) {
                (false, [clip]) => (None, *clip),
                (true, [alias, clip]) => (Some((*alias).to_owned()), *clip),
                _ => return Err("invalid animation state entry"),
            };
            entries.push(AnimStateClip {
                alias,
                clip: clip.to_owned(),
            });
        } else {
            let (name, properties) = line.split_once(':').unwrap_or((line, ""));
            if pending.is_some() || name.trim().is_empty() {
                return Err("animation state body missing");
            }
            pending = Some((
                name.trim().to_owned(),
                properties.split_whitespace().map(str::to_owned).collect(),
            ));
        }
    }
    if pending.is_some() || active.is_some() {
        return Err("unterminated animation state");
    }
    Ok(table)
}

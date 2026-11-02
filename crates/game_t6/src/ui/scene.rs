use super::Axis;

mod front_end;
mod stats;
use super::elements::Elements;
use super::script::runtime::{Bindings, Callback, Runtime};
use super::script::tables::Tables;
use super::script::{Environment, FaultKind, Program, Value};
use game_api::ui::{Content, Draw, Input, Module, ScriptUi};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

fn key(value: &str) -> Value {
    Value::Bytes(Arc::from(value.as_bytes()))
}

fn text(value: &Value) -> Result<String, FaultKind> {
    match value {
        Value::Bytes(value) => {
            String::from_utf8(value.to_vec()).map_err(|_| FaultKind::BindingRequired)
        }
        Value::Number(value) => Ok(value.to_string()),
        Value::Boolean(value) => Ok((*value != 0).to_string()),
        Value::Nil => Ok("nil".into()),
        _ => Err(FaultKind::BindingRequired),
    }
}

pub(super) fn number(value: Option<&Value>) -> Result<f32, FaultKind> {
    match value {
        Some(Value::Number(value)) => Ok(*value),
        Some(Value::Bytes(bytes)) => core::str::from_utf8(bytes)
            .ok()
            .and_then(|text| text.trim().parse::<f32>().ok())
            .ok_or(FaultKind::NumberRequired),
        _ => Err(FaultKind::NumberRequired),
    }
}

fn map_compass(bytes: &[u8]) -> Option<(String, f32)> {
    use gsc_t6::{Opcode, decode_function};
    let module = gsc_t6::Module::parse(bytes).ok()?;
    let mut material = None;
    let mut range = None;
    for export in module.exports.iter().filter(|export| export.name == "main") {
        let end = module
            .exports
            .iter()
            .filter(|next| next.code > export.code)
            .map(|next| next.code as usize)
            .min()
            .unwrap_or(module.code.end);
        let instructions = decode_function(bytes, export.code as usize, end).ok()?;
        let mut literals = Vec::new();
        for instruction in instructions {
            if instruction.opcode == Opcode::PreScriptCall as u8 {
                literals.clear();
            } else if instruction.opcode == Opcode::GetString as u8 {
                let offset = (instruction.operand.start + 1) & !1;
                let value = module
                    .strings
                    .iter()
                    .find(|value| value.refs.contains(&(offset as u32)))?;
                literals.push(value.text.clone());
            } else if let Some(call) = module
                .imports
                .iter()
                .find(|call| call.refs.contains(&(instruction.at as u32)))
            {
                if call.name == "setupminimap" && call.params == 1 && literals.len() == 1 {
                    material = literals.first().cloned();
                } else if call.name == "setdvar"
                    && call.params == 2
                    && literals.len() == 2
                    && literals[1] == "compassmaxrange"
                {
                    range = literals[0]
                        .parse::<f32>()
                        .ok()
                        .filter(|range| range.is_finite() && *range > 0.0);
                }
                literals.clear();
            } else {
                literals.clear();
            }
        }
    }
    Some((material?, range?))
}

#[derive(Debug)]
pub struct Scene {
    runtime: Runtime,
    bindings: Host,
    module_names: Vec<String>,
    /// The scene runs the front end (`T6.main`) instead of the match HUD.
    front_end: bool,
    root: Option<Value>,
    scoreboard: Option<Value>,
    rewards: Option<Value>,
    refreshed: Option<(bool, bool, i32)>,
    last_time: Option<i32>,
    rounds_played: Option<i32>,
    competitive_scores: Vec<(u32, i32)>,
    weapon_state: Option<(String, String, i32)>,
    ammo_state: Option<(i32, i32, bool)>,
    offhand_state: Option<[game_api::ui::OffhandSlot; 2]>,
    events: BTreeMap<String, Value>,
    reward_data: Vec<game_api::ui::Reward>,
}

impl Scene {
    fn bootstrap_hud(&mut self, input: &Input) -> Result<(), String> {
        self.bindings.input = input.clone();
        // The multiplayer executable starts in a session without the
        // zombies mode, which the modules read as they load (`CoD.isZombie`).
        if !input.zombies {
            self.bindings.front_end.session &= !(1 << front_end::SESSION_ZOMBIES);
        }
        self.module("ui/t6/codroot.lua")?;
        self.module("ui/t6/codbase.lua")?;
        self.module("ui_mp/t6/hud.lua")?;
        let cod = self
            .runtime
            .tables
            .global(&key("LUI"))
            .map_err(|e| format!("{e:?}"))?;
        let alignment = self
            .runtime
            .tables
            .field(&cod, &key("Alignment"))
            .map_err(|e| format!("{e:?}"))?;
        for (index, name) in ["Left", "Center", "Right"].into_iter().enumerate() {
            self.bindings.alignments[index] = number(Some(
                &self
                    .runtime
                    .tables
                    .field(&alignment, &key(name))
                    .map_err(|e| format!("{e:?}"))?,
            ))
            .map_err(|e| format!("{e:?}"))? as u8;
        }
        let root = self
            .invoke(&["LUI", "CoDRoot", "new"], &[key("UIRootFull")])?
            .first()
            .cloned()
            .ok_or("t6.ui.render: root missing")?;
        let state = &mut self
            .bindings
            .elements
            .element_mut(&root)
            .map_err(|e| format!("{e:?}"))?
            .current;
        state.horizontal = Axis {
            anchor_start: true,
            anchor_end: true,
            start: 0.0,
            end: 0.0,
        };
        state.vertical = state.horizontal;
        let controller = Value::Number(input.local_client as f32);
        // A multiplayer match draws Black Ops 2's own HUD constructor only
        // when asked (`IW4L_T6_LUI_HUD`): it has run offline, not on screen.
        let full_hud = input.zombies || std::env::var_os("IW4L_T6_LUI_HUD").is_some();
        if !full_hud {
            let safe_area = self
                .invoke(&["LUI", "UIElement", "new"], &[])?
                .first()
                .cloned()
                .ok_or("t6.ui.render: safe area missing")?;
            self.runtime
                .tables
                .set_field(&root, &key("safeArea"), safe_area.clone())
                .map_err(|e| format!("{e:?}"))?;
            let state = &mut self
                .bindings
                .elements
                .element_mut(&safe_area)
                .map_err(|e| format!("{e:?}"))?
                .current;
            state.horizontal = Axis {
                anchor_start: true,
                anchor_end: true,
                start: 0.0,
                end: 0.0,
            };
            state.vertical = state.horizontal;
            self.add(&root, &safe_area)?;
            self.invoke(
                &["CoD", "Compass", "AddMinimap"],
                std::slice::from_ref(&root),
            )?;
            let rewards = self
                .invoke(
                    &["LUI", "createMenu", "RewardSelection"],
                    std::slice::from_ref(&controller),
                )?
                .first()
                .cloned()
                .ok_or("t6.ui.render: rewards missing")?;
            self.add(&safe_area, &rewards)?;
            self.rewards = Some(rewards);
            let scoreboard = self
                .invoke(
                    &["LUI", "createMenu", "Scoreboard"],
                    std::slice::from_ref(&controller),
                )?
                .first()
                .cloned()
                .ok_or("t6.ui.render: scoreboard missing")?;
            self.add(&safe_area, &scoreboard)?;
            self.scoreboard = Some(scoreboard);
        } else {
            let hud = self
                .invoke(
                    &["LUI", "createMenu", "HUD"],
                    std::slice::from_ref(&controller),
                )?
                .first()
                .cloned()
                .ok_or("t6.ui.render: hud missing")?;
            self.add(&root, &hud)?;
            self.event(&hud, "processEvent", "first_snapshot")?;
            let scoreboard = self
                .runtime
                .tables
                .field(&hud, &key("scoreBoard"))
                .map_err(|e| format!("{e:?}"))?;
            if scoreboard != Value::Nil {
                self.add(&hud, &scoreboard)?;
                self.scoreboard = Some(scoreboard);
            }
        }
        self.root = Some(root.clone());
        self.bindings.front_end.root = Some(root);
        self.bindings.zero_profile_settings();
        self.bindings.front_end.session = 1 | (u32::from(input.zombies) << 4);
        self.bindings.front_end.client_used = true;
        self.bindings.front_end.client_primary = true;
        self.bindings.front_end.party = true;
        self.bindings.front_end.game_lobby = true;
        self.sync_match_dvars(input);
        Ok(())
    }

    fn sync_match_dvars(&mut self, input: &Input) {
        let group = self.bindings.zm_gamemode_group().to_owned();
        let start = self.bindings.zm_start_location().to_owned();
        for (name, value) in [
            ("sv_running", "1"),
            ("ui_mapname", input.map.as_str()),
            ("mapname", input.map.as_str()),
            ("ui_gametype", input.gametype.as_str()),
            ("g_gametype", input.gametype.as_str()),
            ("ui_zm_gamemodegroup", group.as_str()),
            ("ui_zm_mapstartlocation", start.as_str()),
        ] {
            let _ = self.bindings.set_dvar(name, &key(value));
        }
        if self.bindings.front_end.gametype != input.gametype {
            let configs = &self.bindings.configs;
            self.bindings.front_end.gametype_settings = crate::gametype_settings(
                &|name: &str| configs.get(&name.to_ascii_lowercase()).map(String::as_str),
                &input.gametype,
            );
            self.bindings.front_end.gametype = input.gametype.clone();
        }
    }

    fn add(&mut self, parent: &Value, child: &Value) -> Result<(), String> {
        let method = self
            .runtime
            .tables
            .field(parent, &key("addElement"))
            .map_err(|e| format!("{e:?}"))?;
        self.runtime
            .call_with(
                &method,
                &[parent.clone(), child.clone()],
                1_000_000,
                &mut self.bindings,
            )
            .map_err(|e| format!("{e:?}"))?;
        Ok(())
    }

    fn invoke(&mut self, path: &[&str], args: &[Value]) -> Result<Vec<Value>, String> {
        let mut function = self.runtime.tables.globals();
        for segment in path {
            function = self
                .runtime
                .tables
                .field(&function, &key(segment))
                .map_err(|e| format!("t6.ui.engine_api: {}: {e:?}", path.join(".")))?;
        }
        self.runtime
            .call_with(&function, args, 1_000_000, &mut self.bindings)
            .map_err(|e| {
                format!(
                    "t6.ui.render: {}: {e:?}; stack {}; binding {}",
                    path.join("."),
                    self.failure_trace(),
                    self.bindings.last_call
                )
            })
    }
    pub fn new(modules: &[Module]) -> Result<Self, String> {
        let mut runtime = Runtime::default();
        runtime.report_pcall_errors = std::env::var_os("IW4L_LUI_TRACE").is_some();
        let elements = Elements::install(&mut runtime.tables)
            .map_err(|e| format!("t6.ui.engine_api: {e:?}"))?;
        let mut host = Host {
            elements,
            modules: BTreeMap::new(),
            loaded: BTreeSet::new(),
            callback: None,
            functions: Vec::new(),
            input: Input::default(),
            last_call: String::new(),
            nodes: BTreeMap::new(),
            assets: Vec::new(),
            force_mouse_root: false,
            maps: BTreeMap::new(),
            alignments: [0; 3],
            front_end: front_end::FrontEnd::default(),
            configs: BTreeMap::new(),
            trace: std::env::var_os("IW4L_LUI_TRACE").is_some(),
        };
        let mut module_names = Vec::new();
        for (name, bytes) in modules {
            if name.ends_with(".gsc") {
                if let Some(map) = map_compass(bytes) {
                    let stem = name
                        .rsplit('/')
                        .next()
                        .unwrap_or(name)
                        .trim_end_matches(".gsc");
                    host.maps.insert(stem.to_owned(), map);
                }
                continue;
            }
            if name.ends_with(".cfg") {
                host.configs.insert(
                    name.to_ascii_lowercase(),
                    String::from_utf8_lossy(bytes).into_owned(),
                );
                continue;
            }
            if !name.ends_with(".lua") {
                continue;
            }
            module_names.push(name.clone());
            let program = fastfile_t6::hks::parse(bytes)
                .map_err(|e| format!("t6.ui.bootstrap: {name}: {e:?}"))?;
            let closure = runtime
                .load(Program::from(program))
                .map_err(|e| format!("t6.ui.bootstrap: {name}: {e:?}"))?;
            host.modules.insert(name.to_ascii_lowercase(), closure);
        }
        host.install(&mut runtime.tables)
            .map_err(|e| format!("t6.ui.engine_api: {e:?}"))?;
        let mut scene = Self {
            runtime,
            bindings: host,
            module_names,
            front_end: false,
            root: None,
            scoreboard: None,
            rewards: None,
            refreshed: None,
            last_time: None,
            rounds_played: None,
            competitive_scores: Vec::new(),
            weapon_state: None,
            ammo_state: None,
            offhand_state: None,
            events: BTreeMap::new(),
            reward_data: Vec::new(),
        };
        scene.module("ui/lui/lui.lua")?;
        let tables = &mut scene.runtime.tables;
        let lui = tables
            .global(&key("LUI"))
            .map_err(|e| format!("t6.ui.engine_api: {e:?}"))?;
        let class = tables
            .field(&lui, &key("UIElement"))
            .map_err(|e| format!("t6.ui.engine_api: {e:?}"))?;
        let container = tables
            .global(&key("UIContainer"))
            .map_err(|e| format!("t6.ui.engine_api: {e:?}"))?;
        for (key, value) in tables
            .entries(&container)
            .map_err(|e| format!("t6.ui.engine_api: {e:?}"))?
        {
            if tables
                .raw_get(&class, &key)
                .map_err(|e| format!("t6.ui.engine_api: {e:?}"))?
                == Value::Nil
            {
                tables
                    .raw_set(&class, &key, value)
                    .map_err(|e| format!("t6.ui.engine_api: {e:?}"))?;
            }
        }
        scene
            .bindings
            .install_front_end(&mut scene.runtime.tables)
            .map_err(|e| format!("t6.ui.engine_api: {e:?}"))?;
        Ok(scene)
    }

    /// Where the last fault happened: each active frame's module, function
    /// and pc, innermost first.
    fn failure_trace(&self) -> String {
        self.runtime
            .failure_stack()
            .iter()
            .map(|(program, function, pc)| {
                format!(
                    "{} fn {function} pc {pc}",
                    self.module_names.get(*program).map_or("?", String::as_str)
                )
            })
            .collect::<Vec<_>>()
            .join(" <- ")
    }

    /// Logs the script errors protected calls caught, naming each
    /// program by its module.
    fn report_errors(&mut self) {
        for error in std::mem::take(&mut self.runtime.errors) {
            let mut named = error;
            for (index, name) in self.module_names.iter().enumerate() {
                named = named.replace(&format!("program {index} fn"), &format!("{name} fn"));
            }
            diag::warn!(Ui, "lui error: {named}");
        }
    }

    fn module(&mut self, name: &str) -> Result<(), String> {
        let closure = self
            .bindings
            .modules
            .get(name)
            .cloned()
            .ok_or_else(|| format!("t6.ui.bootstrap: module {name} missing"))?;
        self.bindings.loaded.insert(name.into());
        self.runtime
            .call_with(&closure, &[], 1_000_000, &mut self.bindings)
            .map_err(|e| {
                let source = self
                    .runtime
                    .failure_source()
                    .map(|(program, function)| (self.module_names.get(program), function));
                format!(
                    "t6.ui.bootstrap: {name}: {e:?}; source {source:?}; binding {}",
                    self.bindings.last_call
                )
            })?;
        Ok(())
    }
}

impl Scene {
    fn dispatch_event(
        &mut self,
        receiver: &Value,
        method: &str,
        name: &str,
        event: Value,
    ) -> Result<(), String> {
        if !self.front_end
            && method == "processEvent"
            && self.root.as_ref() == Some(receiver)
            && self.runtime.tables.raw_get(&event, &key("immediate")) == Ok(Value::Nil)
        {
            let _ = self
                .runtime
                .tables
                .raw_set(&event, &key("immediate"), Value::Boolean(1));
        }
        let callee = self
            .runtime
            .tables
            .field(receiver, &key(method))
            .map_err(|e| format!("{e:?}"))?;
        self.runtime
            .call_with(
                &callee,
                &[receiver.clone(), event],
                1_000_000,
                &mut self.bindings,
            )
            .map_err(|e| {
                format!(
                    "t6.ui.render: {method} {name}: {e:?}; stack {}; binding {}",
                    self.failure_trace(),
                    self.bindings.last_call
                )
            })?;
        Ok(())
    }

    fn event(&mut self, receiver: &Value, method: &str, name: &str) -> Result<(), String> {
        let event = if let Some(value) = self.events.get(name) {
            value.clone()
        } else {
            let value = self
                .runtime
                .tables
                .allocate()
                .map_err(|e| format!("{e:?}"))?;
            self.events.insert(name.into(), value.clone());
            value
        };
        self.runtime
            .tables
            .raw_set(&event, &key("name"), key(name))
            .map_err(|e| format!("{e:?}"))?;
        self.runtime
            .tables
            .raw_set(
                &event,
                &key("controller"),
                Value::Number(self.bindings.input.local_client as f32),
            )
            .map_err(|e| format!("{e:?}"))?;
        self.dispatch_event(receiver, method, name, event)
    }

    fn update_zombies_hud(
        &mut self,
        root: &Value,
        input: &Input,
        refresh: bool,
    ) -> Result<(), String> {
        let controller = Value::Number(input.local_client as f32);
        for cf in &input.client_fields {
            let event = self
                .runtime
                .tables
                .allocate()
                .map_err(|e| format!("{e:?}"))?;
            for (field, value) in [
                ("name", key(&cf.name)),
                ("controller", controller.clone()),
                ("wasDemoJump", Value::Boolean(0)),
                ("initialSnap", Value::Boolean(cf.initial.into())),
                ("newEnt", Value::Boolean(cf.new_entity.into())),
                ("entNum", Value::Number(cf.entity.unwrap_or(0) as f32)),
                ("oldValue", Value::Number(cf.old)),
                ("newValue", Value::Number(cf.new)),
            ] {
                self.runtime
                    .tables
                    .raw_set(&event, &key(field), value)
                    .map_err(|e| format!("{e:?}"))?;
            }
            self.dispatch_event(root, "processEvent", &cf.name, event)?;
        }
        for notify in &input.lui_notifies {
            let event = self
                .runtime
                .tables
                .allocate()
                .map_err(|e| format!("{e:?}"))?;
            self.runtime
                .tables
                .raw_set(&event, &key("name"), key(&notify.name))
                .map_err(|e| format!("{e:?}"))?;
            self.runtime
                .tables
                .raw_set(&event, &key("controller"), controller.clone())
                .map_err(|e| format!("{e:?}"))?;
            if notify.name == "allow_round_animation" {
                if let Some(&allow) = notify.data.first() {
                    self.runtime
                        .tables
                        .raw_set(&event, &key("allow"), Value::Number(allow as f32))
                        .map_err(|e| format!("{e:?}"))?;
                }
            } else if !notify.data.is_empty() {
                let data = self
                    .runtime
                    .tables
                    .allocate()
                    .map_err(|e| format!("{e:?}"))?;
                for (index, item) in notify.data.iter().enumerate() {
                    self.runtime
                        .tables
                        .raw_set(
                            &data,
                            &Value::Number(index as f32 + 1.0),
                            Value::Number(*item as f32),
                        )
                        .map_err(|e| format!("{e:?}"))?;
                }
                self.runtime
                    .tables
                    .raw_set(&event, &key("data"), data)
                    .map_err(|e| format!("{e:?}"))?;
            }
            self.dispatch_event(root, "processEvent", &notify.name, event)?;
        }
        if input.rounds_played > 0 && (refresh || self.rounds_played != Some(input.rounds_played)) {
            self.rounds_played = Some(input.rounds_played);
            let event = self
                .runtime
                .tables
                .allocate()
                .map_err(|e| format!("{e:?}"))?;
            for (field, value) in [
                ("name", key("hud_update_rounds_played")),
                ("controller", controller.clone()),
                ("roundsPlayed", Value::Number(input.rounds_played as f32)),
                ("wasDemoJump", Value::Boolean(0)),
            ] {
                self.runtime
                    .tables
                    .raw_set(&event, &key(field), value)
                    .map_err(|e| format!("{e:?}"))?;
            }
            self.dispatch_event(root, "processEvent", "hud_update_rounds_played", event)?;
        }
        let scores: Vec<(u32, i32)> = input.players.iter().map(|p| (p.id, p.score)).collect();
        if !scores.is_empty() && (refresh || self.competitive_scores != scores) {
            self.competitive_scores = scores;
            let list = self
                .runtime
                .tables
                .allocate()
                .map_err(|e| format!("{e:?}"))?;
            for (index, player) in input.players.iter().enumerate() {
                let entry = self
                    .runtime
                    .tables
                    .allocate()
                    .map_err(|e| format!("{e:?}"))?;
                self.runtime
                    .tables
                    .raw_set(&entry, &key("score"), Value::Number(player.score as f32))
                    .map_err(|e| format!("{e:?}"))?;
                self.runtime
                    .tables
                    .raw_set(&entry, &key("clientNum"), Value::Number(player.id as f32))
                    .map_err(|e| format!("{e:?}"))?;
                self.runtime
                    .tables
                    .raw_set(&list, &Value::Number(index as f32 + 1.0), entry)
                    .map_err(|e| format!("{e:?}"))?;
            }
            let self_index = input
                .players
                .iter()
                .position(|p| p.id == input.local_client)
                .unwrap_or(0)
                + 1;
            let event = self
                .runtime
                .tables
                .allocate()
                .map_err(|e| format!("{e:?}"))?;
            for (field, value) in [
                ("name", key("hud_update_competitive_scoreboard")),
                ("controller", controller.clone()),
                ("bWasDemoJump", Value::Boolean(0)),
                ("competitivescores", list),
                ("selfindex", Value::Number(self_index as f32)),
            ] {
                self.runtime
                    .tables
                    .raw_set(&event, &key(field), value)
                    .map_err(|e| format!("{e:?}"))?;
            }
            self.dispatch_event(
                root,
                "processEvent",
                "hud_update_competitive_scoreboard",
                event,
            )?;
        }
        let weapon_state = (
            input.weapon_name.clone(),
            input.weapon_display_name.clone(),
            input.weapon_inventory_type,
        );
        if !input.weapon_name.is_empty()
            && (refresh || self.weapon_state.as_ref() != Some(&weapon_state))
        {
            self.weapon_state = Some(weapon_state);
            let select = self
                .runtime
                .tables
                .allocate()
                .map_err(|e| format!("{e:?}"))?;
            for (field, value) in [
                ("name", key("hud_update_weapon_select")),
                ("controller", controller.clone()),
                ("weaponDisplayName", key(&input.weapon_display_name)),
            ] {
                self.runtime
                    .tables
                    .raw_set(&select, &key(field), value)
                    .map_err(|e| format!("{e:?}"))?;
            }
            self.dispatch_event(root, "processEvent", "hud_update_weapon_select", select)?;
            let weap = self
                .runtime
                .tables
                .allocate()
                .map_err(|e| format!("{e:?}"))?;
            for (field, value) in [
                ("name", key("hud_update_weapon")),
                ("controller", controller.clone()),
                ("weapon", key(&input.weapon_name)),
                (
                    "inventorytype",
                    Value::Number(input.weapon_inventory_type as f32),
                ),
                ("immediate", Value::Boolean(1)),
            ] {
                self.runtime
                    .tables
                    .raw_set(&weap, &key(field), value)
                    .map_err(|e| format!("{e:?}"))?;
            }
            self.dispatch_event(root, "processEvent", "hud_update_weapon", weap)?;
        }
        let ammo_state = (input.ammo_clip, input.ammo_stock, input.low_clip);
        if !input.weapon_name.is_empty() && (refresh || self.ammo_state != Some(ammo_state)) {
            self.ammo_state = Some(ammo_state);
            let ammo = self
                .runtime
                .tables
                .allocate()
                .map_err(|e| format!("{e:?}"))?;
            for (field, value) in [
                ("name", key("hud_update_ammo")),
                ("controller", controller.clone()),
                ("ammoInClip", Value::Number(input.ammo_clip as f32)),
                ("ammoStock", Value::Number(input.ammo_stock as f32)),
                ("lowClip", Value::Boolean(input.low_clip.into())),
                ("immediate", Value::Boolean(1)),
            ] {
                self.runtime
                    .tables
                    .raw_set(&ammo, &key(field), value)
                    .map_err(|e| format!("{e:?}"))?;
            }
            self.dispatch_event(root, "processEvent", "hud_update_ammo", ammo)?;
        }
        if refresh || self.offhand_state.as_ref() != Some(&input.offhand) {
            self.offhand_state = Some(input.offhand.clone());
            let lethal = self
                .runtime
                .tables
                .allocate()
                .map_err(|e| format!("{e:?}"))?;
            let lethal_mat = if input.offhand[0].material.is_empty() {
                Value::Nil
            } else {
                self.bindings.asset(&input.offhand[0].material)
            };
            for (field, value) in [
                ("ammo", Value::Number(input.offhand[0].ammo as f32)),
                ("material", lethal_mat),
                ("aspectRatio", Value::Number(1.0)),
            ] {
                self.runtime
                    .tables
                    .raw_set(&lethal, &key(field), value)
                    .map_err(|e| format!("{e:?}"))?;
            }
            let tactical = self
                .runtime
                .tables
                .allocate()
                .map_err(|e| format!("{e:?}"))?;
            let tactical_mat = if input.offhand[1].material.is_empty() {
                Value::Nil
            } else {
                self.bindings.asset(&input.offhand[1].material)
            };
            for (field, value) in [
                ("ammo", Value::Number(input.offhand[1].ammo as f32)),
                ("material", tactical_mat),
            ] {
                self.runtime
                    .tables
                    .raw_set(&tactical, &key(field), value)
                    .map_err(|e| format!("{e:?}"))?;
            }
            let event = self
                .runtime
                .tables
                .allocate()
                .map_err(|e| format!("{e:?}"))?;
            for (field, value) in [
                ("name", key("hud_update_offhand")),
                ("controller", controller),
                ("lethal", lethal),
                ("tactical", tactical),
            ] {
                self.runtime
                    .tables
                    .raw_set(&event, &key(field), value)
                    .map_err(|e| format!("{e:?}"))?;
            }
            self.dispatch_event(root, "processEvent", "hud_update_offhand", event)?;
        }
        Ok(())
    }
}

impl ScriptUi for Scene {
    fn take_requests(&mut self) -> Vec<game_api::ui::UiRequest> {
        std::mem::take(&mut self.bindings.front_end.requests)
    }

    fn menu_open(&self) -> bool {
        self.bindings.front_end.ingame_menu_open
    }

    fn update(&mut self, input: &Input) -> Result<Vec<Draw>, String> {
        let mut input = input.clone();
        if input.gametype.is_empty() {
            input.gametype = if input.zombies {
                "zclassic"
            } else if input.team_game {
                "tdm"
            } else {
                "dm"
            }
            .into();
        }
        let input = &input;
        if self.root.is_none() {
            if self.front_end {
                self.bootstrap_front_end(input)?;
            } else {
                self.bootstrap_hud(input)?;
            }
        }
        self.bindings.input = input.clone();
        let root = self.root.clone().ok_or("t6.ui.render: root missing")?;
        if self.front_end {
            return self.update_front_end(&root, input);
        }
        self.sync_match_dvars(input);
        self.bindings.run_queued();
        self.send_input(&root, input)?;
        self.send(&root, "process_events", &[("immediate", Value::Boolean(1))])?;
        if self.reward_data != input.rewards
            && let Some(rewards) = self.rewards.clone()
        {
            let values = self
                .runtime
                .tables
                .allocate()
                .map_err(|e| format!("{e:?}"))?;
            for (index, reward) in input.rewards.iter().take(3).enumerate() {
                let value = self
                    .runtime
                    .tables
                    .allocate()
                    .map_err(|e| format!("{e:?}"))?;
                let material = self.bindings.asset(&reward.material);
                for (name, field) in [
                    ("momentumCost", Value::Number(reward.cost as f32)),
                    ("material", material),
                    ("ammo", Value::Number(reward.quantity as f32)),
                    ("itemName", key(&reward.label)),
                    ("slotIndex", Value::Number(index as f32 + 1.0)),
                ] {
                    self.runtime
                        .tables
                        .raw_set(&value, &key(name), field)
                        .map_err(|e| format!("{e:?}"))?;
                }
                self.runtime
                    .tables
                    .raw_set(&values, &Value::Number(index as f32 + 1.0), value)
                    .map_err(|e| format!("{e:?}"))?;
            }
            self.invoke(
                &["CoD", "RewardSelection", "UpdateRewards"],
                &[rewards, values],
            )?;
            self.reward_data = input.rewards.clone();
        }
        let menu_active = input.menu || self.bindings.front_end.ingame_menu_open;
        let refresh = self.refreshed.is_none_or(|(scoreboard, menu, _)| {
            scoreboard != input.scoreboard || menu != menu_active
        });
        if refresh {
            self.event(&root, "dispatchEventToChildren", "hud_update_refresh")?;
        }
        if input.zombies {
            self.update_zombies_hud(&root, input, refresh)?;
        }
        if let Some(scoreboard) = self.scoreboard.clone() {
            self.bindings
                .elements
                .element_mut(&scoreboard)
                .map_err(|e| format!("{e:?}"))?
                .current
                .opacity = (input.scoreboard && !menu_active) as u8 as f32;
            if input.scoreboard
                && !menu_active
                && (refresh
                    || self
                        .refreshed
                        .is_none_or(|(_, _, time)| input.time.wrapping_sub(time) >= 100))
            {
                self.event(&scoreboard, "processEvent", "update_scoreboard")?;
                self.refreshed = Some((input.scoreboard, menu_active, input.time));
            }
        }
        if refresh {
            self.refreshed = Some((input.scoreboard, menu_active, input.time));
        }
        self.paint(&root, input)
    }
}

impl Scene {
    /// The scene's draws this frame, advancing its animations to `input.time`.
    fn paint(&mut self, root: &Value, input: &Input) -> Result<Vec<Draw>, String> {
        self.report_errors();
        let elapsed = self
            .last_time
            .map_or(0, |prev| input.time.wrapping_sub(prev).max(0));
        self.last_time = Some(input.time);
        let width = input.viewport[0] / (input.viewport[1] / 720.0).max(0.001);
        let mut draws = Vec::new();
        self.draw(
            identity(root).map_err(|e| format!("{e:?}"))?,
            [0.0, 0.0, width, 720.0],
            [1.0; 4],
            None,
            elapsed,
            0,
            &mut draws,
        )?;
        Ok(draws)
    }
}

#[derive(Debug)]
struct Host {
    elements: Elements,
    modules: BTreeMap<String, Value>,
    loaded: BTreeSet<String>,
    callback: Option<Callback>,
    functions: Vec<String>,
    input: Input,
    last_call: String,
    nodes: BTreeMap<u32, Node>,
    assets: Vec<String>,
    force_mouse_root: bool,
    maps: BTreeMap<String, (String, f32)>,
    alignments: [u8; 3],
    front_end: front_end::FrontEnd,
    /// The zones' config files (`.cfg`), by name without case.
    configs: BTreeMap<String, String>,
    /// `IW4L_LUI_TRACE`: log every engine call the scripts make.
    trace: bool,
}

#[derive(Clone, Debug, Default)]
struct Node {
    parent: Option<u32>,
    children: Vec<u32>,
    kind: Option<Content>,
    priority: i32,
    list_axis: Option<u8>,
    stencil: bool,
    focus: bool,
    owner: Option<u32>,
}

fn identity(value: &Value) -> Result<u32, FaultKind> {
    let Value::Reference { kind: 7, identity } = value else {
        return Err(FaultKind::BindingRequired);
    };
    Ok(*identity)
}

impl Scene {
    #[allow(clippy::too_many_arguments)]
    fn draw(
        &mut self,
        id: u32,
        parent: [f32; 4],
        parent_color: [f32; 4],
        clip: Option<[f32; 4]>,
        elapsed: i32,
        depth: usize,
        draws: &mut Vec<Draw>,
    ) -> Result<(), String> {
        if depth > 128 {
            return Err(format!("t6.ui.render: {:?}", FaultKind::IndexChain));
        }
        let receiver = Value::Reference {
            kind: 7,
            identity: id,
        };
        let step = {
            let element = self
                .bindings
                .elements
                .element_mut(&receiver)
                .map_err(|e| format!("t6.ui.render: {e:?}"))?;
            if let (Some(source), Some(target), Some(timer)) = (
                element.animation_source.as_mut(),
                element.animation_target.as_mut(),
                element.animation_timer.as_mut(),
            ) {
                source.resolve_rectangle(Some(parent));
                target.resolve_rectangle(Some(parent));
                let step = timer.advance(elapsed);
                if let super::animation::AnimationStep::Interpolate(fraction) = step {
                    let source = *source;
                    let target = *target;
                    element.current.interpolate(&source, &target, fraction);
                }
                Some(step)
            } else {
                element.current.resolve_rectangle(Some(parent));
                None
            }
        };
        if let Some(super::animation::AnimationStep::Complete { .. }) = step {
            self.bindings
                .elements
                .finish_element_animation(&receiver, false, &mut self.runtime.tables)
                .map_err(|e| format!("t6.ui.render: {e:?}"))?;
            while let Some(callback) = self.bindings.take_callback() {
                self.runtime
                    .call_with(
                        &callback.callee,
                        &callback.arguments,
                        1_000_000,
                        &mut self.bindings,
                    )
                    .map_err(|e| {
                        format!(
                            "t6.ui.render: transition_complete: {e:?}; source {:?}; binding {}",
                            self.runtime.failure_source().map(|(program, function)| (
                                self.module_names.get(program),
                                function
                            )),
                            self.bindings.last_call
                        )
                    })?;
            }
            let element = self
                .bindings
                .elements
                .element_mut(&receiver)
                .map_err(|e| format!("t6.ui.render: {e:?}"))?;
            if let (Some(source), Some(target)) = (
                element.animation_source.as_mut(),
                element.animation_target.as_mut(),
            ) {
                source.resolve_rectangle(Some(parent));
                target.resolve_rectangle(Some(parent));
                let source = *source;
                let target = *target;
                element.current.interpolate(&source, &target, 0.0);
            } else {
                element.current.resolve_rectangle(Some(parent));
            }
        }
        let Some(node) = self.bindings.nodes.get(&id).cloned() else {
            return Ok(());
        };
        let state = self
            .bindings
            .elements
            .element(&receiver)
            .map_err(|e| format!("t6.ui.render: {e:?}"))?
            .current;
        let mut rectangle = state.resolved_rectangle;
        if state.scale_delta != 0.0 {
            let scale = 1.0 + state.scale_delta;
            let cx = (rectangle[0] + rectangle[2]) * 0.5;
            let cy = (rectangle[1] + rectangle[3]) * 0.5;
            let hw = (rectangle[2] - rectangle[0]) * 0.5 * scale;
            let hh = (rectangle[3] - rectangle[1]) * 0.5 * scale;
            rectangle = [cx - hw, cy - hh, cx + hw, cy + hh];
        }
        let color = [
            state.color[0] * parent_color[0],
            state.color[1] * parent_color[1],
            state.color[2] * parent_color[2],
            state.opacity * parent_color[3],
        ];
        let clip = if node.stencil {
            Some(clip.map_or(rectangle, |clip| {
                [
                    rectangle[0].max(clip[0]),
                    rectangle[1].max(clip[1]),
                    rectangle[2].min(clip[2]),
                    rectangle[3].min(clip[3]),
                ]
            }))
        } else {
            clip
        };
        if let Some(mut content) = node.kind {
            match &mut content {
                Content::Image(name) => {
                    *name = state
                        .material
                        .and_then(|id| self.bindings.assets.get(id as usize - 1))
                        .cloned()
                        .unwrap_or_else(String::new)
                }
                Content::Globe { material, rotation } => {
                    *material = state
                        .material
                        .and_then(|id| self.bindings.assets.get(id as usize - 1))
                        .cloned()
                        .unwrap_or_else(String::new);
                    *rotation = state.shader_vectors[2].unwrap_or([0.0; 4]);
                }
                Content::Text { font, .. } => {
                    *font = state
                        .font
                        .and_then(|id| self.bindings.assets.get(id as usize - 1))
                        .cloned()
                        .unwrap_or_else(|| "fonts/distFont".into())
                }
                Content::GameTimer => {
                    let remaining = self.bindings.input.remaining.unwrap_or(0);
                    let value = if remaining <= 0 {
                        String::new()
                    } else if remaining < 30_000 {
                        format!("{}.{}", remaining / 1000, (remaining / 100) % 10)
                    } else {
                        format!("{}:{:02}", remaining / 60_000, (remaining / 1000) % 60)
                    };
                    content = Content::Text {
                        value,
                        font: state
                            .font
                            .and_then(|id| self.bindings.assets.get(id as usize - 1))
                            .cloned()
                            .unwrap_or_else(|| "fonts/distFont".into()),
                    };
                }
                Content::Compass {
                    material, range, ..
                } => {
                    if let Some((name, max_range)) =
                        self.bindings.maps.get(&self.bindings.input.map)
                    {
                        *material = name.clone();
                        *range = *max_range;
                    }
                }
                Content::PlayerEmblem(_) | Content::Voice(_) => {}
            }
            if color[3] > 0.0 {
                draws.push(Draw {
                    id,
                    rectangle,
                    color,
                    rotation: state.rotation[2],
                    alignment: self
                        .bindings
                        .alignments
                        .iter()
                        .position(|value| Some(*value) == state.alignment)
                        .unwrap_or(0) as u8,
                    clip,
                    content,
                    text_shadow: true,
                });
            }
        }
        let mut children = node.children;
        children.sort_by_key(|id| self.bindings.nodes.get(id).map_or(0, |node| node.priority));
        let mut cursor = 0.0;
        for child in children {
            let mut parent = rectangle;
            if let Some(axis) = node.list_axis {
                let axis = axis as usize;
                parent[axis] += cursor;
                parent[axis + 2] += cursor;
            }
            self.draw(child, parent, color, clip, elapsed, depth + 1, draws)?;
            if let Some(axis) = node.list_axis {
                let state = self
                    .bindings
                    .elements
                    .element(&Value::Reference {
                        kind: 7,
                        identity: child,
                    })
                    .map_err(|e| format!("t6.ui.render: {e:?}"))?
                    .current;
                let axis = axis as usize;
                cursor += state.resolved_rectangle[axis + 2] - state.resolved_rectangle[axis]
                    + self
                        .bindings
                        .elements
                        .element(&receiver)
                        .map_err(|e| format!("t6.ui.render: {e:?}"))?
                        .current
                        .spacing;
            }
        }
        Ok(())
    }
}

impl Host {
    fn zm_default_row(&self, column: usize) -> Option<&str> {
        let table = self
            .input
            .tables
            .iter()
            .find(|table| table.name.eq_ignore_ascii_case("zm/gametypestable.csv"))?;
        if table.columns <= column.max(7) {
            return None;
        }
        let starts = || {
            table
                .cells
                .chunks_exact(table.columns)
                .filter(|row| row[0] == "6" && row[2].eq_ignore_ascii_case(&self.input.map))
        };
        // The match's gametype and location, else the map's default start.
        let picked = || {
            starts().filter(|row| {
                row[4] == self.input.gametype
                    && (self.input.location.is_empty() || row[3] == self.input.location)
            })
        };
        picked()
            .find(|row| row[7] == "YES")
            .or_else(|| picked().next())
            .or_else(|| starts().find(|row| row[7] == "YES"))
            .map(|row| row[column].as_str())
    }

    /// The start's gametype's mode group: the last column of its gametype
    /// row.
    fn zm_gamemode_group(&self) -> &str {
        let group = self.zm_default_row(4).and_then(|gametype| {
            let table = self
                .input
                .tables
                .iter()
                .find(|table| table.name.eq_ignore_ascii_case("zm/gametypestable.csv"))?;
            table
                .cells
                .chunks_exact(table.columns.max(1))
                .find(|row| {
                    row.first().is_some_and(|c| c == "0")
                        && row.get(1).is_some_and(|c| c == gametype)
                })?
                .get(9)
                .map(String::as_str)
        });
        group.unwrap_or("zclassic")
    }

    fn zm_start_location(&self) -> &str {
        self.zm_default_row(3).unwrap_or("")
    }

    fn asset(&mut self, name: &str) -> Value {
        let index = self
            .assets
            .iter()
            .position(|asset| asset == name)
            .unwrap_or_else(|| {
                self.assets.push(name.into());
                self.assets.len() - 1
            });
        Value::Reference {
            kind: 2,
            identity: index as u32 + 1,
        }
    }

    fn bind(&mut self, tables: &mut Tables, table: &Value, name: &str) -> Result<(), FaultKind> {
        let identity = self.functions.len() as u32 + 100;
        self.functions.push(name.into());
        tables.raw_set(table, &key(name), Value::Reference { kind: 10, identity })
    }

    fn install(&mut self, tables: &mut Tables) -> Result<(), FaultKind> {
        let globals = tables.globals();
        tables.raw_set(&globals, &key("_G"), globals.clone())?;
        for name in [
            "require",
            "pairs",
            "ipairs",
            "ipairs_next",
            "next",
            "tostring",
            "tonumber",
            "unpack",
            "error",
            "DebugPrint",
            "RegisterFont",
            "RegisterMaterial",
            "GetTextDimensions",
            "ProjectRootCoordinate",
        ] {
            self.bind(tables, &globals, name)?;
        }
        let debug = tables.allocate()?;
        self.bind(tables, &debug, "postdeploymentfunction")?;
        tables.raw_set(&globals, &key("debug"), debug)?;
        let container = tables.allocate()?;
        Elements::bind_methods(tables, &container)?;
        for name in [
            "setClass",
            "setLeftRight",
            "setTopBottom",
            "setRGB",
            "setAlpha",
            "setXRot",
            "setYRot",
            "setZRot",
            "setScale",
            "setZoom",
            "setSpacing",
            "setUI3DWindow",
            "setBlur",
            "setTileHorizontally",
            "setTileVertically",
            "getLocalRect",
            "getRect",
            "getHeight",
            "RootPixelsToUnits",
            "addElementToC",
            "addElementBeforeInC",
            "addElementAfterInC",
            "getParent",
            "getNumChildren",
            "setOwner",
            "setOwnerControllerIndex",
            "getOwner",
            "setUseGameTime",
            "setHandleMouse",
            "setupUIElement",
            "setupUIImage",
            "setupUIStreamedImage",
            "setupUIText",
            "setupUITextUncached",
            "setupUITightText",
            "setupBitchinFX",
            "setupSafeAreaBoundary",
            "setupGameMessages",
            "setupLoadingBar",
            "setupLoadingStatusText",
            "setupEntityContainer",
            "setEntityContainerClamp",
            "setEntityContainerFadeWhenTargeted",
            "setEntityContainerStopUpdating",
            "setEntityContainerScale",
            "setEntityContainerScaleClamp",
            "setImage",
            "setFont",
            "setAlignment",
            "setShaderVector",
            "getShaderVector2",
            "setTextInC",
            "setRoot",
            "setPriority",
            "getPriority",
            "setupUIHorizontalList",
            "setupUIVerticalList",
            "setUseStencil",
            "getUseStencil",
            "setupHorizontalCompass",
            "setupCompassUnderlay",
            "setupCompassItems",
            "setupCompassOverlay",
            "getFirstChild",
            "getLastChild",
            "getNextSibling",
            "getPreviousSibling",
            "removeElement",
            "removeAllChildren",
            "setupGameTimer",
            "setupGameTimerZombie",
            "setupPlayerEmblemServer",
            "setupVoipImage",
            "setupIngameTalkers",
            "beginAnimation",
            "completeAnimation",
            "isInFocus",
            "setFocus",
        ] {
            self.bind(tables, &container, name)?;
        }
        tables.raw_set(&globals, &key("UIContainer"), container)?;
        for table_name in ["Engine", "UIExpression"] {
            let table = tables.allocate()?;
            for name in [
                "GetCurrentPlatform",
                "GetCurrentExe",
                "SessionMode_IsZombiesGame",
                "RegisterFont",
                "Localize",
                "TableLookup",
                "IsUsingCursor",
                "Exec",
                "PlayMenuMusic",
                "SetForceMouseRootFull",
                "ForceHUDRefresh",
                "SetDvar",
                "Stop3DCinematic",
                "IsCinematicWebm",
                "GetUserSafeArea",
                "GetLanguage",
                "GetDvarBool",
                "DvarBool",
                "IsInGame",
                "GameModeIsMode",
                "GetDvarString",
                "DvarString",
                "GetDvarInt",
                "DvarInt",
                "GetGametypeSetting",
                "StopEditingPresetClass",
                "GetUserSafeAreaForController",
                "IsSplitscreen",
                "GetScoreBoardColumnName",
                "IsDemoPlaying",
                "PartyConnectingToDedicated",
                "IsInOvertime",
                "GetRoundsPlayed",
                "IsVisibilityBitSet",
                "IsShoutcaster",
                "IsDemoShoutcaster",
                "IsSpectatingActiveClient",
                "ExeProfileVarBool",
                "LastInput_Gamepad",
                "GetTeamPositions",
                "GetMatchScoreboardClientCount",
                "GetClientNum",
                "IsGuest",
                "SessionModeIsMode",
                "GetObjectiveName",
                "PartyGetPlayerCount",
                "IsPlayerMuteToggled",
                "IsDemoClipPlaying",
                "GetMatchScoreboardIndexAndClientNumForTeam",
                "GetFullGamertagForScoreboardIndex",
                "GetScoreboardColumnForScoreboardIndex",
                "GetRankForScoreboardIndex",
                "GetRankIconForScoreboardIndex",
                "GetPrestigeForScoreboardIndex",
                "GetStatusIconForClient",
                "GetPingForScoreboardIndex",
                "GetFactionForTeam",
                "GetFactionColor",
                "GetCalloutPlayerData",
                "SessionMode_IsOnlineGame",
                "KeyBinding",
                "GetKeyBindingLocalizedString",
                "IsWeaponType",
                "IsOverheatWeapon",
                "ToUpper",
                "IsMigrating",
                "SetActiveMenu",
                "LockInput",
                "SetUIActive",
            ] {
                if table_name == "UIExpression"
                    && matches!(name, "IsDemoPlaying" | "IsCinematicWebm" | "IsInOvertime")
                {
                    let identity = self.functions.len() as u32 + 100;
                    self.functions.push(format!("uiexpr:{name}"));
                    tables.raw_set(&table, &key(name), Value::Reference { kind: 10, identity })?;
                } else {
                    self.bind(tables, &table, name)?;
                }
            }
            tables.raw_set(&globals, &key(table_name), table)?;
        }
        let dvars = tables.allocate()?;
        for name in [
            "loc_language",
            "g_TeamColor_MyTeam",
            "g_TeamColor_EnemyTeam",
            "ui_mapname",
            "ui_gametype",
            "ui_zm_gamemodegroup",
            "ui_zm_mapstartlocation",
            "ui_zm_useloadingmovie",
            "ui_errorMessage",
            "party_maxplayers",
            "g_gametype",
            "r_dualPlayEnable",
            "r_stereo3DOn",
            "hiDef",
            "wideScreen",
            "tu7_scoreboardPingAsNumbers",
        ] {
            let dvar = tables.allocate()?;
            let identity = self.functions.len() as u32 + 100;
            self.functions.push(format!("dvar:{name}"));
            tables.raw_set(&dvar, &key("get"), Value::Reference { kind: 10, identity })?;
            let set_identity = self.functions.len() as u32 + 100;
            self.functions.push("dvar_set".into());
            tables.raw_set(
                &dvar,
                &key("set"),
                Value::Reference {
                    kind: 10,
                    identity: set_identity,
                },
            )?;
            tables.raw_set(&dvar, &key("dvar_name"), key(name))?;
            tables.raw_set(&dvars, &key(name), dvar)?;
        }
        tables.raw_set(&globals, &key("Dvar"), dvars)?;
        let string = tables.allocate()?;
        for name in [
            "char", "sub", "len", "lower", "upper", "reverse", "find", "match", "gsub",
        ] {
            self.bind(tables, &string, name)?;
        }
        tables.raw_set(&globals, &key("string"), string)?;
        let table = tables.allocate()?;
        for name in ["insert", "remove", "concat", "getn"] {
            self.bind(tables, &table, name)?;
        }
        tables.raw_set(
            &table,
            &key("sort"),
            Value::Reference {
                kind: 10,
                identity: super::script::library::SORT,
            },
        )?;
        tables.raw_set(&globals, &key("table"), table)?;
        let math = tables.allocate()?;
        for name in [
            "min", "max", "floor", "ceil", "abs", "pow", "sqrt", "sin", "cos", "random",
        ] {
            self.bind(tables, &math, name)?;
        }
        // HavokScript numbers are single precision.
        tables.raw_set(&math, &key("pi"), Value::Number(std::f32::consts::PI))?;
        tables.raw_set(&globals, &key("math"), math)?;
        // Every other name the engine registers fails by its name when
        // called, instead of reaching the scripts' `CoD.NullFunction`.
        for (table, names, label) in [
            ("Engine", super::engine_names::ENGINE, "Engine"),
            (
                "UIExpression",
                super::engine_names::EXPRESSIONS,
                "UIExpression",
            ),
            (
                "UIContainer",
                super::engine_names::ELEMENT_METHODS,
                "UIElement",
            ),
        ] {
            let table = tables.global(&key(table))?;
            for name in names {
                if tables.raw_get(&table, &key(name))? == Value::Nil {
                    let identity = self.functions.len() as u32 + 100;
                    self.functions.push(format!("missing:{label}.{name}"));
                    tables.raw_set(&table, &key(name), Value::Reference { kind: 10, identity })?;
                }
            }
        }
        Ok(())
    }

    fn require(&mut self, args: &[Value]) -> Result<Vec<Value>, FaultKind> {
        let name = text(args.first().ok_or(FaultKind::Argument(1))?)?;
        let suffix = name.replace('.', "/").to_ascii_lowercase() + ".lua";
        let path = [format!("ui/{suffix}"), format!("ui_mp/{suffix}")]
            .into_iter()
            .find(|path| self.modules.contains_key(path))
            .ok_or(FaultKind::Unknown(super::script::ENGINE_API))?;
        if self.loaded.insert(path.clone()) {
            self.last_call = format!("require {path}");
            self.callback = Some(Callback {
                callee: self.modules[&path].clone(),
                arguments: Vec::new(),
            });
        }
        Ok(vec![Value::Boolean(1)])
    }
}

impl Bindings for Host {
    fn take_callback(&mut self) -> Option<Callback> {
        self.callback
            .take()
            .or_else(|| self.elements.take_callback())
            .or_else(|| self.front_end.callbacks.pop_front())
    }

    fn call(
        &mut self,
        function: u32,
        args: &[Value],
        tables: &mut Tables,
    ) -> Result<Vec<Value>, FaultKind> {
        if function < 100 {
            let result = self.elements.call(function, args, tables)?;
            if function == 7 {
                self.nodes.insert(
                    identity(result.first().ok_or(FaultKind::BindingRequired)?)?,
                    Node::default(),
                );
            } else if function == 8
                && let Some(Value::Reference { kind: 7, identity }) = args.first()
                && let Some(parent) = self.nodes.remove(identity).and_then(|node| node.parent)
                && let Some(parent_node) = self.nodes.get_mut(&parent)
            {
                parent_node.children.retain(|id| *id != *identity);
            }
            return Ok(result);
        }
        let name = self
            .functions
            .get((function - 100) as usize)
            .cloned()
            .ok_or(FaultKind::BindingRequired)?;
        self.last_call = format!("{name} {args:?}");
        if self.trace {
            let shown: Vec<String> = args
                .iter()
                .map(|arg| match arg {
                    Value::Bytes(bytes) => format!("{:?}", String::from_utf8_lossy(bytes)),
                    other => format!("{other:?}"),
                })
                .collect();
            diag::info!(Ui, "lui call: {name}({})", shown.join(", "));
        }
        if let Some(result) = self.front_end_call(&name, args, tables) {
            return result;
        }
        match name.as_str() {
            "min" | "max" => {
                let mut result = number(args.first())?;
                for arg in &args[1..] {
                    let value = number(Some(arg))?;
                    if (name == "min" && value < result) || (name == "max" && value > result) {
                        result = value;
                    }
                }
                Ok(vec![Value::Number(result)])
            }
            "floor" => Ok(vec![Value::Number(number(args.first())?.floor())]),
            "ceil" => Ok(vec![Value::Number(number(args.first())?.ceil())]),
            "abs" => Ok(vec![Value::Number(number(args.first())?.abs())]),
            "pow" => Ok(vec![Value::Number(
                number(args.first())?.powf(number(args.get(1))?),
            )]),
            "sqrt" => Ok(vec![Value::Number(number(args.first())?.sqrt())]),
            "sin" => Ok(vec![Value::Number(number(args.first())?.sin())]),
            "cos" => Ok(vec![Value::Number(number(args.first())?.cos())]),
            "random" => Ok(vec![Value::Number(match (args.first(), args.get(1)) {
                (None, _) => 0.5,
                (Some(a), None) => 1.0_f32.min(number(Some(a))?.floor()),
                (Some(a), Some(_)) => number(Some(a))?.floor(),
            })]),
            "ToUpper" => Ok(vec![key(&text(
                args.last().ok_or(FaultKind::Argument(1))?,
            )?
            .to_uppercase())]),
            "lower" => Ok(vec![key(&text(
                args.first().ok_or(FaultKind::Argument(1))?,
            )?
            .to_lowercase())]),
            "upper" => Ok(vec![key(&text(
                args.first().ok_or(FaultKind::Argument(1))?,
            )?
            .to_uppercase())]),
            "char" => Ok(vec![Value::Bytes(Arc::from(
                args.iter()
                    .map(|value| {
                        let value = number(Some(value))?;
                        if value.fract() != 0.0 || !(0.0..=255.0).contains(&value) {
                            return Err(FaultKind::Argument(1));
                        }
                        Ok(value as u8)
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            ))]),
            "GetCurrentPlatform" => Ok(vec![key("pc")]),
            "GetTextDimensions" => {
                let source = text(args.first().ok_or(FaultKind::Argument(1))?)?;
                let Value::Reference { kind: 2, identity } =
                    args.get(1).ok_or(FaultKind::Argument(2))?
                else {
                    return Err(FaultKind::BindingRequired);
                };
                let name = self
                    .assets
                    .get(*identity as usize - 1)
                    .ok_or(FaultKind::BindingRequired)?;
                let font = self
                    .input
                    .fonts
                    .iter()
                    .find(|font| font.name.eq_ignore_ascii_case(name))
                    .ok_or(FaultKind::Unknown(super::script::ENGINE_API))?;
                let size = number(args.get(2))?;
                let mut width = 0.0_f32;
                let mut line = 0.0_f32;
                let mut chars = source.chars().peekable();
                while let Some(character) = chars.next() {
                    if character == '^' && chars.peek().is_some_and(|c| c.is_ascii_digit()) {
                        chars.next();
                        continue;
                    }
                    if character == '\n' {
                        width = width.max(line);
                        line = 0.0;
                        continue;
                    }
                    let advance = font
                        .advances
                        .iter()
                        .find(|(code, _)| *code == character as u16)
                        .or_else(|| font.advances.iter().find(|(code, _)| *code == b'?' as u16))
                        .ok_or(FaultKind::Unknown(super::script::ENGINE_API))?
                        .1;
                    line += advance;
                }
                width = width.max(line) * size / font.height;
                if let Some(Value::Number(max)) = args.get(3)
                    && *max >= 0.0
                {
                    width = *max;
                }
                Ok(vec![
                    Value::Number(0.0),
                    Value::Number(size),
                    Value::Number(width),
                    Value::Number(0.0),
                ])
            }
            "IsUsingCursor" => Ok(vec![Value::Boolean(self.input.menu.into())]),
            "SetForceMouseRootFull" => {
                self.force_mouse_root = args.last().is_some_and(Value::truthy);
                Ok(Vec::new())
            }
            "Exec" if args.last() == Some(&key("ui_keyboard_cancel")) => Ok(Vec::new()),
            "StopEditingPresetClass"
            | "ForceHUDRefresh"
            | "SetDvar"
            | "Stop3DCinematic"
            | "SetActiveMenu"
            | "LockInput"
            | "SetUIActive"
            | "dvar_set" => Ok(Vec::new()),
            binding if binding.starts_with("uiexpr:") => Ok(vec![Value::Number(0.0)]),
            "TableLookup" => {
                if args.len() < 5 || args.len().is_multiple_of(2) {
                    return Err(FaultKind::Argument(args.len()));
                }
                let name = text(args.get(1).ok_or(FaultKind::Argument(2))?)?;
                let mut pairs = Vec::new();
                for pair in args[2..args.len() - 1].as_chunks::<2>().0 {
                    pairs.push((number(Some(&pair[0]))? as usize, text(&pair[1])?));
                }
                let result = number(args.last())? as usize;
                let value = self
                    .input
                    .tables
                    .iter()
                    .find(|table| table.name.eq_ignore_ascii_case(&name))
                    .filter(|table| {
                        table.columns > result && pairs.iter().all(|(col, _)| table.columns > *col)
                    })
                    .and_then(|table| {
                        table
                            .cells
                            .chunks_exact(table.columns)
                            .find(|row| pairs.iter().all(|(col, needle)| row[*col] == *needle))
                            .map(|row| row[result].as_str())
                    })
                    .unwrap_or("");
                Ok(vec![key(value)])
            }
            "Localize" => {
                let name = text(args.first().ok_or(FaultKind::Argument(1))?)?;
                let value = self
                    .input
                    .localized
                    .iter()
                    .find(|(key, _)| key.eq_ignore_ascii_case(&name))
                    .map_or(name.as_str(), |(_, value)| value.as_str());
                let mut value = value.to_owned();
                for (index, argument) in args.iter().skip(1).enumerate() {
                    let replacement = text(argument)?;
                    value = value.replace(&format!("&&{}", index + 1), &replacement);
                }
                Ok(vec![key(&value)])
            }
            "GetCurrentExe" => Ok(vec![key("multiplayer")]),
            "SessionMode_IsZombiesGame" => {
                Ok(vec![Value::Number(f32::from(u8::from(self.input.zombies)))])
            }
            "RegisterFont" | "RegisterMaterial" => {
                let name = text(args.first().ok_or(FaultKind::Argument(1))?)?;
                let index = self
                    .assets
                    .iter()
                    .position(|asset| asset == &name)
                    .unwrap_or_else(|| {
                        self.assets.push(name);
                        self.assets.len() - 1
                    });
                Ok(vec![Value::Reference {
                    kind: 2,
                    identity: index as u32 + 1,
                }])
            }
            binding if binding.starts_with("dvar:") => {
                let name = binding.trim_start_matches("dvar:").to_owned();
                Ok(match name.as_str() {
                    "loc_language" => vec![Value::Number(0.0)],
                    "r_dualPlayEnable" | "r_stereo3DOn" | "ui_zm_useloadingmovie" => {
                        vec![Value::Boolean(0)]
                    }
                    "hiDef" | "wideScreen" | "tu7_scoreboardPingAsNumbers" => {
                        vec![Value::Boolean(1)]
                    }
                    "party_maxplayers" => vec![Value::Number(4.0)],
                    "ui_errorMessage" => vec![key("")],
                    "ui_mapname" => vec![key(&self.input.map)],
                    "ui_gametype" | "g_gametype" => vec![key(&self.input.gametype)],
                    "ui_zm_gamemodegroup" => vec![key(self.zm_gamemode_group())],
                    "ui_zm_mapstartlocation" => vec![key(self.zm_start_location())],
                    "g_TeamColor_MyTeam" => {
                        [0.4, 0.7, 0.4].into_iter().map(Value::Number).collect()
                    }
                    "g_TeamColor_EnemyTeam" => {
                        [1.0, 0.315, 0.35].into_iter().map(Value::Number).collect()
                    }
                    _ => return Err(FaultKind::Unknown(super::script::ENGINE_API)),
                })
            }
            "GetDvarString" | "DvarString" => {
                let name = text(args.last().ok_or(FaultKind::Argument(1))?)?;
                match name.as_str() {
                    "ui_mapname" | "mapname" => Ok(vec![key(&self.input.map)]),
                    "ui_gametype" | "g_gametype" => Ok(vec![key(&self.input.gametype)]),
                    "ui_zm_gamemodegroup" => Ok(vec![key(self.zm_gamemode_group())]),
                    "ui_zm_mapstartlocation" => Ok(vec![key(self.zm_start_location())]),
                    _ => Err(FaultKind::Unknown(super::script::ENGINE_API)),
                }
            }
            "GetDvarInt" | "DvarInt" => {
                let name = text(args.last().ok_or(FaultKind::Argument(1))?)?;
                match name.as_str() {
                    "r_fontResolution" => Ok(vec![Value::Number(720.0)]),
                    _ => Err(FaultKind::Unknown(super::script::ENGINE_API)),
                }
            }
            "GetLanguage" => Ok(vec![Value::Number(0.0)]),
            "GetUserSafeArea" => {
                let scale = self.input.viewport[1] / 720.0;
                let area = if self.input.safe_area[0] > 0.0 {
                    self.input.safe_area
                } else {
                    [self.input.viewport[0] / scale, 720.0]
                };
                Ok(vec![Value::Number(area[0]), Value::Number(area[1])])
            }
            "GetUserSafeAreaForController" => {
                let scale = self.input.viewport[1] / 720.0;
                let area = if self.input.safe_area[0] > 0.0 {
                    self.input.safe_area
                } else {
                    [self.input.viewport[0] / scale, 720.0]
                };
                Ok(
                    [-area[0] * 0.5, -area[1] * 0.5, area[0] * 0.5, area[1] * 0.5]
                        .into_iter()
                        .map(Value::Number)
                        .collect(),
                )
            }
            "IsSplitscreen"
            | "IsDemoPlaying"
            | "IsCinematicWebm"
            | "IsMigrating"
            | "PartyConnectingToDedicated"
            | "IsInOvertime"
            | "IsShoutcaster"
            | "IsDemoShoutcaster"
            | "IsSpectatingActiveClient"
            | "LastInput_Gamepad"
            | "IsGuest"
            | "IsPlayerMuteToggled"
            | "IsDemoClipPlaying"
            | "SessionMode_IsOnlineGame"
            | "IsWeaponType"
            | "IsOverheatWeapon" => Ok(vec![Value::Boolean(0)]),
            "KeyBinding" | "GetKeyBindingLocalizedString" => Ok(vec![key("")]),
            "IsVisibilityBitSet" => {
                let bit = args.last().ok_or(FaultKind::Argument(2))?;
                let cod = tables.global(&key("CoD"))?;
                let enabled = if tables.field(&cod, &key("BIT_HUD_VISIBLE"))? == *bit
                    || tables.field(&cod, &key("BIT_COMPASS_VISIBLE"))? == *bit
                {
                    !self.front_end.ingame_menu_open
                } else if tables.field(&cod, &key("BIT_SCOREBOARD_OPEN"))? == *bit {
                    self.input.scoreboard && !self.front_end.ingame_menu_open
                } else if tables.field(&cod, &key("BIT_UI_ACTIVE"))? == *bit {
                    self.input.menu || self.front_end.ingame_menu_open
                } else {
                    false
                };
                Ok(vec![Value::Number(if enabled { 1.0 } else { 0.0 })])
            }
            "ExeProfileVarBool" => {
                let name = text(args.last().ok_or(FaultKind::Argument(1))?)?;
                match name.as_str() {
                    "shoutcaster_minimap" | "shoutcaster_scorestreaks" => {
                        Ok(vec![Value::Number(0.0)])
                    }
                    _ => Err(FaultKind::Unknown(super::script::ENGINE_API)),
                }
            }
            // An empty slot (state 0) gives nothing; a used one its name.
            "GetObjectiveName" => {
                let index = number(args.get(1))? as u32;
                Ok(self
                    .input
                    .objectives
                    .iter()
                    .find(|objective| objective.index == index && objective.state != 0)
                    .map_or_else(Vec::new, |objective| vec![key(&objective.name)]))
            }
            "SessionModeIsMode" => {
                let mode = args.last().ok_or(FaultKind::Argument(1))?;
                let cod = tables.global(&key("CoD"))?;
                Ok(vec![Value::Boolean(
                    (tables.field(&cod, &key("SESSIONMODE_OFFLINE"))? == *mode).into(),
                )])
            }
            "PartyGetPlayerCount" => Ok(vec![Value::Number(self.input.players.len() as f32)]),
            "GetMatchScoreboardIndexAndClientNumForTeam" => {
                let index = number(args.first())? as usize;
                let team = number(args.get(1))? as i32;
                let player = self
                    .input
                    .players
                    .iter()
                    .enumerate()
                    .filter(|(_, player)| player.team == team)
                    .nth(index);
                Ok(player.map_or_else(
                    || vec![Value::Nil, Value::Nil],
                    |(index, player)| {
                        vec![Value::Number(index as f32), Value::Number(player.id as f32)]
                    },
                ))
            }
            "GetFullGamertagForScoreboardIndex"
            | "GetScoreboardColumnForScoreboardIndex"
            | "GetRankForScoreboardIndex"
            | "GetPrestigeForScoreboardIndex"
            | "GetRankIconForScoreboardIndex"
            | "GetPingForScoreboardIndex" => {
                let index = number(args.first())? as usize;
                let player = self
                    .input
                    .players
                    .get(index)
                    .ok_or(FaultKind::Argument(2))?;
                Ok(vec![match name.as_str() {
                    "GetFullGamertagForScoreboardIndex" => key(&player.name),
                    "GetRankForScoreboardIndex" => Value::Number(player.rank as f32),
                    "GetPrestigeForScoreboardIndex" => Value::Number(player.prestige as f32),
                    "GetRankIconForScoreboardIndex" => {
                        let rank = player.rank.to_string();
                        let column = usize::try_from(player.prestige)
                            .ok()
                            .and_then(|prestige| prestige.checked_add(1));
                        let material = self
                            .input
                            .tables
                            .iter()
                            .find(|table| table.name.eq_ignore_ascii_case("mp/rankicontable.csv"))
                            .and_then(|table| {
                                let column = column.filter(|column| *column < table.columns)?;
                                table
                                    .cells
                                    .chunks_exact(table.columns)
                                    .find(|row| row[0] == rank)
                                    .map(|row| row[column].clone())
                                    .filter(|material| !material.is_empty())
                            });
                        material.map_or(Value::Nil, |material| self.asset(&material))
                    }
                    "GetPingForScoreboardIndex" => {
                        Value::Number(player.ping.map_or(-1.0, |ping| ping as f32))
                    }
                    _ => {
                        let column = number(args.get(1))? as usize;
                        let value = if self.input.zombies {
                            [
                                player.score,
                                player.kills,
                                player.zombie_stats[0],
                                player.zombie_stats[1],
                                player.zombie_stats[2],
                            ]
                        } else {
                            [player.score, player.kills, player.deaths, 0, 0]
                        };
                        if !self.input.zombies && column == 3 {
                            key(&format!(
                                "{:.2}",
                                player.kills as f32 / player.deaths.max(1) as f32
                            ))
                        } else {
                            key(&value.get(column).ok_or(FaultKind::Argument(3))?.to_string())
                        }
                    }
                }])
            }
            "GetCalloutPlayerData" => {
                let id = number(args.last())? as u32;
                let player = self
                    .input
                    .players
                    .iter()
                    .find(|player| player.id == id)
                    .ok_or(FaultKind::Argument(1))?;
                let value = tables.allocate()?;
                tables.raw_set(&value, &key("playerClientNum"), Value::Number(id as f32))?;
                tables.raw_set(&value, &key("playerName"), key(&player.name))?;
                Ok(vec![value])
            }
            "GetStatusIconForClient" => Ok(vec![Value::Nil]),
            "GetFactionForTeam" => Ok(vec![key("")]),
            "GetFactionColor" => Ok(vec![
                Value::Number(1.0),
                Value::Number(1.0),
                Value::Number(1.0),
            ]),
            "GetTeamPositions" => {
                let rows = tables.allocate()?;
                let teams: BTreeSet<i32> = self
                    .input
                    .players
                    .iter()
                    .map(|player| player.team)
                    .collect();
                for (index, team) in teams.into_iter().enumerate() {
                    let row = tables.allocate()?;
                    tables.raw_set(&row, &key("team"), Value::Number(team as f32))?;
                    tables.raw_set(&rows, &Value::Number(index as f32 + 1.0), row)?;
                }
                Ok(vec![rows])
            }
            "GetMatchScoreboardClientCount" => {
                let team = number(args.last())? as i32;
                Ok(vec![Value::Number(
                    self.input
                        .players
                        .iter()
                        .filter(|player| player.team == team)
                        .count() as f32,
                )])
            }
            "GetClientNum" => Ok(vec![Value::Number(self.input.local_client as f32)]),
            "GetRoundsPlayed" => Ok(vec![Value::Number(self.input.rounds_played as f32)]),
            "GetGametypeSetting" => {
                let setting = text(args.last().ok_or(FaultKind::Argument(1))?)?;
                match setting.as_str() {
                    "teamCount" => Ok(vec![Value::Number(if self.input.gametype == "dm" {
                        1.0
                    } else {
                        2.0
                    })]),
                    "roundLimit" | "startRound" => Ok(vec![Value::Number(1.0)]),
                    _ => Err(FaultKind::Unknown(super::script::ENGINE_API)),
                }
            }
            "GetScoreBoardColumnName" => {
                let index = number(args.last())? as usize;
                let names = if self.input.zombies {
                    [
                        "MPUI_SCORE",
                        "MPUI_KILLS",
                        "MPUI_DOWNS",
                        "MPUI_REVIVES",
                        "MPUI_HEADSHOTS",
                    ]
                } else {
                    ["MPUI_SCORE", "MPUI_KILLS", "MPUI_DEATHS", "MPUI_RATIO", ""]
                };
                let name = names.get(index).copied().unwrap_or("");
                Ok(vec![key(name)])
            }
            "GameModeIsMode" => Ok(vec![Value::Boolean(0)]),
            "GetDvarBool" | "DvarBool" => {
                let name = text(args.last().ok_or(FaultKind::Argument(1))?)?;
                match name.as_str() {
                    "ui_zm" => Ok(vec![Value::Boolean(self.input.zombies.into())]),
                    _ => Err(FaultKind::Unknown(super::script::ENGINE_API)),
                }
            }
            "require" => self.require(args),
            "DebugPrint" => Ok(Vec::new()),
            "postdeploymentfunction" => {
                self.callback = Some(Callback {
                    callee: args.first().cloned().ok_or(FaultKind::Argument(1))?,
                    arguments: Vec::new(),
                });
                Ok(Vec::new())
            }
            "tostring" => Ok(vec![key(&text(
                args.first().ok_or(FaultKind::Argument(1))?,
            )?)]),
            "tonumber" => Ok(vec![
                text(args.first().ok_or(FaultKind::Argument(1))?)?
                    .parse::<f32>()
                    .map(Value::Number)
                    .unwrap_or(Value::Nil),
            ]),
            "error" => Err(FaultKind::ScriptError(
                text(args.first().ok_or(FaultKind::Argument(1))?)?.into_bytes(),
            )),
            "pairs" | "ipairs" => {
                let iterator = if name == "ipairs" {
                    "ipairs_next"
                } else {
                    "next"
                };
                let index = self
                    .functions
                    .iter()
                    .position(|name| name == iterator)
                    .ok_or(FaultKind::BindingRequired)?;
                Ok(vec![
                    Value::Reference {
                        kind: 10,
                        identity: index as u32 + 100,
                    },
                    args.first().cloned().ok_or(FaultKind::Argument(1))?,
                    if name == "ipairs" {
                        Value::Number(0.0)
                    } else {
                        Value::Nil
                    },
                ])
            }
            "ipairs_next" => {
                let table = args.first().ok_or(FaultKind::Argument(1))?;
                let index = number(args.get(1))? + 1.0;
                let value = tables.raw_get(table, &Value::Number(index))?;
                Ok(if value == Value::Nil {
                    vec![Value::Nil]
                } else {
                    vec![Value::Number(index), value]
                })
            }
            "next" => {
                let entries = tables.entries(args.first().ok_or(FaultKind::Argument(1))?)?;
                let previous = args.get(1).unwrap_or(&Value::Nil);
                let start = if *previous == Value::Nil {
                    0
                } else {
                    entries
                        .iter()
                        .position(|(key, _)| key == previous)
                        .ok_or(FaultKind::KeyRequired)?
                        + 1
                };
                // A key set to nil while the table is traversed stays in place,
                // so the traversal goes on past it; nil values are skipped.
                Ok(entries[start.min(entries.len())..]
                    .iter()
                    .find(|(_, value)| *value != Value::Nil)
                    .map_or_else(
                        || vec![Value::Nil],
                        |(key, value)| vec![key.clone(), value.clone()],
                    ))
            }
            "len" => Ok(vec![Value::Number(
                text(args.first().ok_or(FaultKind::Argument(1))?)?.len() as f32,
            )]),
            "reverse" => {
                let mut bytes = match args.first().ok_or(FaultKind::Argument(1))? {
                    Value::Bytes(bytes) => bytes.to_vec(),
                    other => text(other)?.into_bytes(),
                };
                bytes.reverse();
                Ok(vec![Value::Bytes(bytes.into())])
            }
            "gsub" => {
                // `str_gsub`: replaces up to `n` matches with the
                // replacement text (`%0`-`%9` the captures, `%%` a percent);
                // returns the text and the count.
                let source = match args.first().ok_or(FaultKind::Argument(1))? {
                    Value::Bytes(bytes) => bytes.clone(),
                    other => Arc::from(text(other)?.into_bytes()),
                };
                let pattern = match args.get(1).ok_or(FaultKind::Argument(2))? {
                    Value::Bytes(bytes) => bytes.clone(),
                    other => Arc::from(text(other)?.into_bytes()),
                };
                let replacement = match args.get(2).ok_or(FaultKind::Argument(3))? {
                    Value::Bytes(bytes) => bytes.clone(),
                    Value::Number(_) => Arc::from(text(&args[2])?.into_bytes()),
                    _ => return Err(FaultKind::Unknown(super::script::ENGINE_API)),
                };
                let limit = match args.get(3) {
                    Some(Value::Number(limit)) => *limit as usize,
                    _ => source.len() + 1,
                };
                let anchor = pattern.first() == Some(&b'^');
                let pattern = if anchor { &pattern[1..] } else { &pattern[..] };
                let fail = |error: super::script::pattern::PatternError| {
                    FaultKind::ScriptError(error.0.into())
                };
                let mut out = Vec::new();
                let mut at = 0usize;
                let mut count = 0usize;
                while count < limit {
                    match super::script::pattern::match_at(&source, pattern, at).map_err(fail)? {
                        Some((end, captures)) => {
                            count += 1;
                            let capture = |index: usize| -> Result<Vec<u8>, FaultKind> {
                                if index == 0 {
                                    return Ok(source[at..end].to_vec());
                                }
                                match captures.get(index - 1) {
                                    Some(super::script::pattern::Capture::Text(from, to)) => {
                                        Ok(source[*from..*to].to_vec())
                                    }
                                    Some(super::script::pattern::Capture::Position(position)) => {
                                        Ok((position + 1).to_string().into_bytes())
                                    }
                                    None if index == 1 && captures.is_empty() => {
                                        Ok(source[at..end].to_vec())
                                    }
                                    None => Err(FaultKind::ScriptError(
                                        b"invalid capture index".to_vec(),
                                    )),
                                }
                            };
                            let mut chars = replacement.iter().copied();
                            while let Some(byte) = chars.next() {
                                if byte != b'%' {
                                    out.push(byte);
                                    continue;
                                }
                                match chars.next() {
                                    Some(digit) if digit.is_ascii_digit() => {
                                        out.extend(capture((digit - b'0') as usize)?);
                                    }
                                    Some(other) => out.push(other),
                                    None => {}
                                }
                            }
                            if end > at {
                                at = end;
                            } else if at < source.len() {
                                out.push(source[at]);
                                at += 1;
                            } else {
                                break;
                            }
                        }
                        None if at < source.len() => {
                            out.push(source[at]);
                            at += 1;
                        }
                        None => break,
                    }
                    if anchor {
                        break;
                    }
                }
                out.extend_from_slice(&source[at.min(source.len())..]);
                Ok(vec![
                    Value::Bytes(Arc::from(out)),
                    Value::Number(count as f32),
                ])
            }
            "find" | "match" => {
                // `str_find_aux`: `init` counts from 1 (negative from the
                // end) and clamps into the text; `find` with `plain`, or a
                // pattern without specials, searches for the text itself.
                let haystack = match args.first().ok_or(FaultKind::Argument(1))? {
                    Value::Bytes(bytes) => bytes.clone(),
                    other => Arc::from(text(other)?.into_bytes()),
                };
                let needle = match args.get(1).ok_or(FaultKind::Argument(2))? {
                    Value::Bytes(bytes) => bytes.clone(),
                    other => Arc::from(text(other)?.into_bytes()),
                };
                let length = haystack.len() as i64;
                let init = match args.get(2) {
                    Some(Value::Nil) | None => 1,
                    value => number(value)? as i64,
                };
                let init = if init < 0 { length + init + 1 } else { init };
                let init = (init - 1).clamp(0, length) as usize;
                let plain = name == "find"
                    && (args.get(3).is_some_and(Value::truthy)
                        || !super::script::pattern::has_specials(&needle));
                if plain {
                    return Ok(
                        match haystack[init..]
                            .windows(needle.len().max(1))
                            .position(|window| needle.is_empty() || window == &needle[..])
                            .map(|at| at + init)
                            .filter(|at| at + needle.len() <= haystack.len())
                        {
                            Some(start) => vec![
                                Value::Number((start + 1) as f32),
                                Value::Number((start + needle.len()) as f32),
                            ],
                            None => vec![Value::Nil],
                        },
                    );
                }
                let found = super::script::pattern::find(&haystack, &needle, init)
                    .map_err(|error| FaultKind::ScriptError(error.0.into()))?;
                let Some((start, end, captures)) = found else {
                    return Ok(vec![Value::Nil]);
                };
                let mut values = Vec::new();
                if name == "find" {
                    values.push(Value::Number((start + 1) as f32));
                    values.push(Value::Number(end as f32));
                } else if captures.is_empty() {
                    values.push(Value::Bytes(Arc::from(&haystack[start..end])));
                }
                values.extend(captures.into_iter().map(|capture| match capture {
                    super::script::pattern::Capture::Text(from, to) => {
                        Value::Bytes(Arc::from(&haystack[from..to]))
                    }
                    super::script::pattern::Capture::Position(at) => Value::Number((at + 1) as f32),
                }));
                Ok(values)
            }
            "sub" => {
                let value = text(args.first().ok_or(FaultKind::Argument(1))?)?;
                let length = value.len() as i32;
                let index = |number: i32| {
                    if number < 0 {
                        length + number + 1
                    } else {
                        number
                    }
                };
                let start = index(number(args.get(1))? as i32).max(1).min(length + 1) as usize - 1;
                let end = index(
                    args.get(2)
                        .map(|value| number(Some(value)))
                        .transpose()?
                        .unwrap_or(-1.0) as i32,
                )
                .max(0)
                .min(length) as usize;
                Ok(vec![Value::Bytes(Arc::from(if start < end {
                    &value.as_bytes()[start..end]
                } else {
                    &[]
                }))])
            }
            "getn" => Ok(vec![Value::Number(
                tables.length(args.first().ok_or(FaultKind::Argument(1))?)? as f32,
            )]),
            "insert" => {
                let table = args.first().ok_or(FaultKind::Argument(1))?;
                let length = tables.length(table)?;
                let (index, value) = if args.len() == 2 {
                    (length + 1, args[1].clone())
                } else {
                    (
                        number(args.get(1))? as usize,
                        args.get(2).cloned().ok_or(FaultKind::Argument(3))?,
                    )
                };
                if index == 0 || index > length + 1 {
                    return Err(FaultKind::Argument(2));
                }
                for slot in (index..=length).rev() {
                    let value = tables.raw_get(table, &Value::Number(slot as f32))?;
                    tables.raw_set(table, &Value::Number((slot + 1) as f32), value)?;
                }
                tables.raw_set(table, &Value::Number(index as f32), value)?;
                Ok(Vec::new())
            }
            "remove" => {
                let table = args.first().ok_or(FaultKind::Argument(1))?;
                let length = tables.length(table)?;
                if length == 0 {
                    return Ok(vec![Value::Nil]);
                }
                let index = args
                    .get(1)
                    .filter(|value| **value != Value::Nil)
                    .map(|value| number(Some(value)))
                    .transpose()?
                    .map_or(length, |value| value as usize);
                if index == 0 || index > length {
                    return Ok(vec![Value::Nil]);
                }
                let removed = tables.raw_get(table, &Value::Number(index as f32))?;
                for slot in index..length {
                    let next = tables.raw_get(table, &Value::Number((slot + 1) as f32))?;
                    tables.raw_set(table, &Value::Number(slot as f32), next)?;
                }
                tables.raw_set(table, &Value::Number(length as f32), Value::Nil)?;
                Ok(vec![removed])
            }
            "concat" => {
                let table = args.first().ok_or(FaultKind::Argument(1))?;
                let separator = args
                    .get(1)
                    .map(text)
                    .transpose()?
                    .unwrap_or_else(String::new);
                let mut values = Vec::new();
                for index in 1..=tables.length(table)? {
                    values.push(text(&tables.raw_get(table, &Value::Number(index as f32))?)?);
                }
                Ok(vec![key(&values.join(&separator))])
            }
            "unpack" => {
                let table = args.first().ok_or(FaultKind::Argument(1))?;
                let mut values = Vec::new();
                for index in 1..=4096 {
                    let value = tables.raw_get(table, &Value::Number(index as f32))?;
                    if value == Value::Nil {
                        break;
                    }
                    values.push(value);
                }
                Ok(values)
            }
            "setClass" => {
                let receiver = args.first().ok_or(FaultKind::Argument(1))?;
                let class = args.get(1).ok_or(FaultKind::Argument(2))?;
                let metatable = tables
                    .metatable(receiver)?
                    .ok_or(FaultKind::BindingRequired)?;
                let fields = tables.raw_get(&metatable, &key("__index"))?;
                let fallback = tables.allocate()?;
                tables.raw_set(&fallback, &key("__index"), class.clone())?;
                tables.set_metatable(&fields, Some(fallback))?;
                Ok(Vec::new())
            }
            "setLeftRight" | "setTopBottom" => {
                let axis = Axis {
                    anchor_start: args.get(1).is_some_and(Value::truthy),
                    anchor_end: args.get(2).is_some_and(Value::truthy),
                    start: number(args.get(3))?,
                    end: number(args.get(4))?,
                };
                let element = self
                    .elements
                    .element_mut(args.first().ok_or(FaultKind::Argument(1))?)?;
                if name == "setLeftRight" {
                    element.set_horizontal(axis);
                } else {
                    element.set_vertical(axis);
                }
                Ok(Vec::new())
            }
            "isInFocus" => {
                let receiver = identity(args.first().ok_or(FaultKind::Argument(1))?)?;
                Ok(vec![Value::Boolean(
                    self.nodes
                        .get(&receiver)
                        .ok_or(FaultKind::Userdata(receiver))?
                        .focus
                        .into(),
                )])
            }
            "setFocus" => {
                let receiver = identity(args.first().ok_or(FaultKind::Argument(1))?)?;
                self.nodes
                    .get_mut(&receiver)
                    .ok_or(FaultKind::Userdata(receiver))?
                    .focus = args.get(1).is_some_and(Value::truthy);
                Ok(Vec::new())
            }
            "setUseStencil" => {
                let receiver = identity(args.first().ok_or(FaultKind::Argument(1))?)?;
                self.nodes
                    .get_mut(&receiver)
                    .ok_or(FaultKind::Userdata(receiver))?
                    .stencil = args.get(1).is_some_and(Value::truthy);
                Ok(Vec::new())
            }
            "setupHorizontalCompass"
            | "setupCompassUnderlay"
            | "setupCompassItems"
            | "setupCompassOverlay" => {
                let receiver = identity(args.first().ok_or(FaultKind::Argument(1))?)?;
                let layer = match name.as_str() {
                    "setupCompassUnderlay" => 0,
                    "setupCompassItems" => 1,
                    "setupCompassOverlay" => 2,
                    _ => 3,
                };
                self.nodes
                    .get_mut(&receiver)
                    .ok_or(FaultKind::Userdata(receiver))?
                    .kind = Some(Content::Compass {
                    material: String::new(),
                    range: 0.0,
                    layer,
                    full: args
                        .get(1)
                        .is_some_and(|value| *value == Value::Number(1.0)),
                });
                Ok(Vec::new())
            }
            "setupPlayerEmblemServer" | "setupVoipImage" => {
                let receiver = identity(args.first().ok_or(FaultKind::Argument(1))?)?;
                let client = number(args.get(1))? as u32;
                self.nodes
                    .get_mut(&receiver)
                    .ok_or(FaultKind::Userdata(receiver))?
                    .kind = Some(if name == "setupPlayerEmblemServer" {
                    Content::PlayerEmblem(client)
                } else {
                    Content::Voice(client)
                });
                Ok(Vec::new())
            }
            "setupGameTimer" | "setupGameTimerZombie" => {
                let receiver = identity(args.first().ok_or(FaultKind::Argument(1))?)?;
                self.nodes
                    .get_mut(&receiver)
                    .ok_or(FaultKind::Userdata(receiver))?
                    .kind = Some(Content::GameTimer);
                Ok(Vec::new())
            }
            "beginAnimation" => {
                let receiver = args.first().ok_or(FaultKind::Argument(1))?;
                self.elements
                    .finish_element_animation(receiver, true, tables)?;
                let name_bytes = match args.get(1) {
                    Some(Value::Bytes(bytes)) => bytes[..bytes
                        .iter()
                        .position(|byte| *byte == 0)
                        .unwrap_or(bytes.len())]
                        .to_vec(),
                    _ => Vec::new(),
                };
                let duration = args
                    .get(2)
                    .filter(|value| **value != Value::Nil)
                    .map(|value| number(Some(value)))
                    .transpose()?
                    .unwrap_or(0.0);
                let duration = (duration as i32).max(0);
                let options = [
                    args.get(3).is_some_and(Value::truthy),
                    args.get(4).is_some_and(Value::truthy),
                ];
                let element = self.elements.element_mut(receiver)?;
                let target = super::State {
                    changes: super::Changes::default(),
                    ..element.current
                };
                element.begin_animation(name_bytes, duration, options, target);
                Ok(Vec::new())
            }
            "completeAnimation" => {
                let receiver = args.first().ok_or(FaultKind::Argument(1))?;
                self.elements
                    .finish_element_animation(receiver, true, tables)?;
                Ok(Vec::new())
            }
            "setScale" | "setZoom" | "setSpacing" | "setXRot" | "setYRot" | "setZRot" => {
                let value = number(args.get(1))?;
                let element = self
                    .elements
                    .element_mut(args.first().ok_or(FaultKind::Argument(1))?)?;
                match name.as_str() {
                    "setScale" => element.set_scale(value),
                    "setZoom" => {
                        let state = element
                            .animation_target
                            .as_mut()
                            .unwrap_or(&mut element.current);
                        state.zoom = value;
                        state.changes.zoom = true;
                    }
                    "setSpacing" => {
                        element.current.spacing = value;
                        element.current.changes.spacing = true;
                    }
                    "setXRot" => {
                        element.current.rotation[0] = value;
                        element.current.changes.rotation[0] = true;
                    }
                    "setYRot" => {
                        element.current.rotation[1] = value;
                        element.current.changes.rotation[1] = true;
                    }
                    _ => element.set_z_rot(value),
                }
                Ok(Vec::new())
            }
            "setUI3DWindow" => {
                let value = number(args.get(1))? as i32;
                let element = self
                    .elements
                    .element_mut(args.first().ok_or(FaultKind::Argument(1))?)?;
                element.current.ui_3d_window = Some(value);
                Ok(Vec::new())
            }
            "setOwner" | "setOwnerControllerIndex" => {
                let receiver = identity(args.first().ok_or(FaultKind::Argument(1))?)?;
                let owner = args
                    .get(1)
                    .filter(|value| **value != Value::Nil)
                    .map(|value| number(Some(value)))
                    .transpose()?
                    .map(|value| value as u32);
                self.nodes
                    .get_mut(&receiver)
                    .ok_or(FaultKind::Userdata(receiver))?
                    .owner = owner;
                Ok(Vec::new())
            }
            "getOwner" => {
                let receiver = identity(args.first().ok_or(FaultKind::Argument(1))?)?;
                let owner = self
                    .nodes
                    .get(&receiver)
                    .ok_or(FaultKind::Userdata(receiver))?
                    .owner
                    .unwrap_or(self.input.local_client);
                Ok(vec![Value::Number(owner as f32)])
            }
            "getPriority" => {
                let receiver = identity(args.first().ok_or(FaultKind::Argument(1))?)?;
                let priority = self
                    .nodes
                    .get(&receiver)
                    .ok_or(FaultKind::Userdata(receiver))?
                    .priority;
                Ok(vec![Value::Number(priority as f32)])
            }
            "getNumChildren" => {
                let receiver = identity(args.first().ok_or(FaultKind::Argument(1))?)?;
                let count = self
                    .nodes
                    .get(&receiver)
                    .ok_or(FaultKind::Userdata(receiver))?
                    .children
                    .len();
                Ok(vec![Value::Number(count as f32)])
            }
            "getUseStencil" => {
                let receiver = identity(args.first().ok_or(FaultKind::Argument(1))?)?;
                let stencil = self
                    .nodes
                    .get(&receiver)
                    .ok_or(FaultKind::Userdata(receiver))?
                    .stencil;
                Ok(vec![Value::Boolean(stencil.into())])
            }
            "getRect" => Ok(self
                .elements
                .element(args.first().ok_or(FaultKind::Argument(1))?)?
                .current
                .resolved_rectangle
                .into_iter()
                .map(Value::Number)
                .collect()),
            "getHeight" => {
                let rect = self
                    .elements
                    .element(args.first().ok_or(FaultKind::Argument(1))?)?
                    .local_rectangle();
                Ok(vec![Value::Number((rect[3] - rect[1]).abs())])
            }
            "ProjectRootCoordinate" => {
                if text(args.first().ok_or(FaultKind::Argument(1))?)? != "UIRootFull" {
                    return Err(FaultKind::Unknown(super::script::ENGINE_API));
                }
                // Mouse events already use the full-screen root's units.
                Ok(vec![
                    Value::Number(number(args.get(1))?),
                    Value::Number(number(args.get(2))?),
                ])
            }
            "RootPixelsToUnits" => Ok(self
                .elements
                .element(args.first().ok_or(FaultKind::Argument(1))?)?
                .root_point_in_units([number(args.get(1))?, number(args.get(2))?])
                .into_iter()
                .map(Value::Number)
                .collect()),
            "setUseGameTime"
            | "setHandleMouse"
            | "setBlur"
            | "setTileHorizontally"
            | "setTileVertically"
            | "setupUIElement"
            | "setupBitchinFX"
            | "setupSafeAreaBoundary"
            | "setupGameMessages"
            | "setupLoadingBar"
            | "setupLoadingStatusText"
            | "setupEntityContainer"
            | "setEntityContainerClamp"
            | "setEntityContainerFadeWhenTargeted"
            | "setEntityContainerStopUpdating"
            | "setEntityContainerScale"
            | "setEntityContainerScaleClamp"
            | "setupIngameTalkers" => Ok(Vec::new()),
            "setRGB" => {
                self.elements
                    .element_mut(args.first().ok_or(FaultKind::Argument(1))?)?
                    .set_color([
                        number(args.get(1))?,
                        number(args.get(2))?,
                        number(args.get(3))?,
                    ]);
                Ok(Vec::new())
            }
            "setAlpha" => {
                self.elements
                    .element_mut(args.first().ok_or(FaultKind::Argument(1))?)?
                    .set_opacity(number(args.get(1))?);
                Ok(Vec::new())
            }
            "getLocalRect" => Ok(self
                .elements
                .element(args.first().ok_or(FaultKind::Argument(1))?)?
                .local_rectangle()
                .into_iter()
                .map(Value::Number)
                .collect()),
            "addElementToC" | "addElementBeforeInC" | "addElementAfterInC" => {
                // `parent:addElementToC(child)` appends; `element:addElement
                // {Before,After}InC(sibling)` puts the element beside the
                // sibling, under its parent, with its priority.
                let first = identity(args.first().ok_or(FaultKind::Argument(1))?)?;
                let second = identity(args.get(1).ok_or(FaultKind::Argument(2))?)?;
                let (parent, child, sibling) = if name == "addElementToC" {
                    (first, second, None)
                } else {
                    let parent = self
                        .nodes
                        .get(&second)
                        .ok_or(FaultKind::Userdata(second))?
                        .parent
                        .ok_or(FaultKind::BindingRequired)?;
                    (parent, first, Some(second))
                };
                let old = self
                    .nodes
                    .get(&child)
                    .ok_or(FaultKind::Userdata(child))?
                    .parent;
                if let Some(old) = old {
                    self.nodes
                        .get_mut(&old)
                        .ok_or(FaultKind::Userdata(old))?
                        .children
                        .retain(|id| *id != child);
                }
                if let Some(sibling) = sibling {
                    let priority = self.nodes[&sibling].priority;
                    self.nodes
                        .get_mut(&child)
                        .ok_or(FaultKind::Userdata(child))?
                        .priority = priority;
                }
                let child_node = self
                    .nodes
                    .get_mut(&child)
                    .ok_or(FaultKind::Userdata(child))?;
                child_node.parent = Some(parent);
                let child_priority = child_node.priority;
                let priority_pos = if sibling.is_none() {
                    self.nodes
                        .get(&parent)
                        .ok_or(FaultKind::Userdata(parent))?
                        .children
                        .iter()
                        .position(|id| {
                            self.nodes
                                .get(id)
                                .is_some_and(|node| node.priority > child_priority)
                        })
                } else {
                    None
                };
                let children = &mut self
                    .nodes
                    .get_mut(&parent)
                    .ok_or(FaultKind::Userdata(parent))?
                    .children;
                match sibling.and_then(|sibling| children.iter().position(|id| *id == sibling)) {
                    Some(pos) if name == "addElementBeforeInC" => children.insert(pos, child),
                    Some(pos) => children.insert(pos + 1, child),
                    None => match priority_pos {
                        Some(pos) => children.insert(pos, child),
                        None => children.push(child),
                    },
                }
                Ok(Vec::new())
            }
            "removeElement" => {
                let parent = identity(args.first().ok_or(FaultKind::Argument(1))?)?;
                let child = identity(args.get(1).ok_or(FaultKind::Argument(2))?)?;
                self.nodes
                    .get_mut(&parent)
                    .ok_or(FaultKind::Userdata(parent))?
                    .children
                    .retain(|id| *id != child);
                self.nodes
                    .get_mut(&child)
                    .ok_or(FaultKind::Userdata(child))?
                    .parent = None;
                Ok(Vec::new())
            }
            "removeAllChildren" => {
                let parent = identity(args.first().ok_or(FaultKind::Argument(1))?)?;
                let children = std::mem::take(
                    &mut self
                        .nodes
                        .get_mut(&parent)
                        .ok_or(FaultKind::Userdata(parent))?
                        .children,
                );
                for child in children {
                    self.nodes
                        .get_mut(&child)
                        .ok_or(FaultKind::Userdata(child))?
                        .parent = None;
                }
                Ok(Vec::new())
            }
            "getFirstChild" | "getLastChild" | "getNextSibling" | "getPreviousSibling" => {
                let receiver = identity(args.first().ok_or(FaultKind::Argument(1))?)?;
                let node = self
                    .nodes
                    .get(&receiver)
                    .ok_or(FaultKind::Userdata(receiver))?;
                let selected = match name.as_str() {
                    "getFirstChild" => node.children.first().copied(),
                    "getLastChild" => node.children.last().copied(),
                    _ => node
                        .parent
                        .and_then(|id| self.nodes.get(&id))
                        .and_then(|parent| {
                            let index = parent.children.iter().position(|id| *id == receiver)?;
                            let index = if name == "getNextSibling" {
                                index.checked_add(1)?
                            } else {
                                index.checked_sub(1)?
                            };
                            parent.children.get(index).copied()
                        }),
                };
                Ok(vec![selected.map_or(Value::Nil, |identity| {
                    Value::Reference { kind: 7, identity }
                })])
            }
            "getParent" => {
                let receiver = identity(args.first().ok_or(FaultKind::Argument(1))?)?;
                Ok(vec![
                    self.nodes
                        .get(&receiver)
                        .and_then(|node| node.parent)
                        .map_or(Value::Nil, |identity| Value::Reference {
                            kind: 7,
                            identity,
                        }),
                ])
            }
            "setupUIImage"
            | "setupUIStreamedImage"
            | "setupUIText"
            | "setupUITextUncached"
            | "setupUITightText" => {
                let receiver = identity(args.first().ok_or(FaultKind::Argument(1))?)?;
                self.nodes
                    .get_mut(&receiver)
                    .ok_or(FaultKind::Userdata(receiver))?
                    .kind = Some(
                    if matches!(name.as_str(), "setupUIImage" | "setupUIStreamedImage") {
                        Content::Image(String::new())
                    } else {
                        Content::Text {
                            value: String::new(),
                            font: String::new(),
                        }
                    },
                );
                Ok(Vec::new())
            }
            "setImage" | "setFont" => {
                let handle = match args.get(1).ok_or(FaultKind::Argument(2))? {
                    Value::Nil => None,
                    Value::Reference { kind: 2, identity } => Some(*identity),
                    _ => return Err(FaultKind::BindingRequired),
                };
                let element = self
                    .elements
                    .element_mut(args.first().ok_or(FaultKind::Argument(1))?)?;
                if name == "setImage" {
                    element.current.material = handle;
                    if let Some(target) = element.animation_target.as_mut() {
                        target.material = handle;
                    }
                } else {
                    element.current.font = handle;
                    if let Some(target) = element.animation_target.as_mut() {
                        target.font = handle;
                    }
                }
                Ok(Vec::new())
            }
            "setAlignment" => {
                let alignment = Some(number(args.get(1))? as u8);
                let element = self
                    .elements
                    .element_mut(args.first().ok_or(FaultKind::Argument(1))?)?;
                element.current.alignment = alignment;
                if let Some(target) = element.animation_target.as_mut() {
                    target.alignment = alignment;
                }
                Ok(Vec::new())
            }
            "getShaderVector2" => {
                let element = self
                    .elements
                    .element(args.first().ok_or(FaultKind::Argument(1))?)?;
                Ok(element.current.shader_vectors[2]
                    .unwrap_or([0.0; 4])
                    .into_iter()
                    .map(Value::Number)
                    .collect())
            }
            "setShaderVector" => {
                let vector = number(args.get(1))? as usize;
                let values = [
                    number(args.get(2))?,
                    number(args.get(3))?,
                    number(args.get(4))?,
                    number(args.get(5))?,
                ];
                let element = self
                    .elements
                    .element_mut(args.first().ok_or(FaultKind::Argument(1))?)?;
                element.set_shader_vector(vector, values)?;
                Ok(Vec::new())
            }
            "setTextInC" => {
                let receiver = identity(args.first().ok_or(FaultKind::Argument(1))?)?;
                let value = text(args.get(1).ok_or(FaultKind::Argument(2))?)?;
                self.nodes
                    .get_mut(&receiver)
                    .ok_or(FaultKind::Userdata(receiver))?
                    .kind = Some(Content::Text {
                    value,
                    font: String::new(),
                });
                Ok(Vec::new())
            }
            "setRoot" => {
                let state = &mut self
                    .elements
                    .element_mut(args.first().ok_or(FaultKind::Argument(1))?)?
                    .current;
                let scale = self.input.viewport[1] / 720.0;
                let width = if scale > 0.0 {
                    self.input.viewport[0] / scale
                } else {
                    1280.0
                };
                state.horizontal = Axis {
                    anchor_start: true,
                    anchor_end: true,
                    start: 0.0,
                    end: width,
                };
                state.vertical = Axis {
                    anchor_start: true,
                    anchor_end: true,
                    start: 0.0,
                    end: 720.0,
                };
                state.resolved_rectangle = [0.0, 0.0, width, 720.0];
                Ok(Vec::new())
            }
            "setPriority" => {
                let receiver = identity(args.first().ok_or(FaultKind::Argument(1))?)?;
                self.nodes
                    .get_mut(&receiver)
                    .ok_or(FaultKind::Userdata(receiver))?
                    .priority = args
                    .get(1)
                    .filter(|value| **value != Value::Nil)
                    .map(|value| number(Some(value)))
                    .transpose()?
                    .unwrap_or(0.0) as i32;
                Ok(Vec::new())
            }
            "setupUIHorizontalList" | "setupUIVerticalList" => {
                let receiver = identity(args.first().ok_or(FaultKind::Argument(1))?)?;
                self.nodes
                    .get_mut(&receiver)
                    .ok_or(FaultKind::Userdata(receiver))?
                    .list_axis = Some(if name == "setupUIHorizontalList" {
                    0
                } else {
                    1
                });
                Ok(Vec::new())
            }
            _ => Err(FaultKind::Unknown(super::script::ENGINE_API)),
        }
    }
}

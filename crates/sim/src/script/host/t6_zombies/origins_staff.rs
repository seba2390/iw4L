use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(super) enum StaffKind {
    Fire,
    Ice,
    Lightning,
    Gas,
}

impl StaffKind {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Fire => "fire",
            Self::Ice => "ice",
            Self::Lightning => "lightning",
            Self::Gas => "gas",
        }
    }

    pub(super) fn model(self) -> &'static str {
        match self {
            Self::Fire => "p6_zm_staff_fire",
            Self::Ice => "p6_zm_staff_ice",
            Self::Lightning => "p6_zm_staff_lightning",
            Self::Gas => "p6_zm_staff_gas",
        }
    }

    pub(super) fn part_name(self) -> &'static str {
        match self {
            Self::Fire => "fire_staff_part",
            Self::Ice => "ice_staff_part",
            Self::Lightning => "lightning_staff_part",
            Self::Gas => "gas_staff_part",
        }
    }

    pub(super) fn from_part_name(name: &str) -> Option<Self> {
        match name {
            "fire_staff_part" => Some(Self::Fire),
            "ice_staff_part" => Some(Self::Ice),
            "lightning_staff_part" => Some(Self::Lightning),
            "gas_staff_part" => Some(Self::Gas),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct Staffs {
    owned: BTreeMap<ClientId, BTreeSet<StaffKind>>,
    parts: BTreeMap<ClientId, BTreeMap<StaffKind, u8>>,
    stations: Vec<StaffStation>,
}

#[derive(Clone, Debug)]
struct StaffStation {
    kind: StaffKind,
    origin: [f32; 3],
    angles: [f32; 3],
    object: Option<u64>,
}

impl Staffs {
    pub(super) fn initialize(&mut self, world: &mut World, authored: &[Vec<(String, String)>]) {
        self.stations = authored
            .iter()
            .filter_map(|row| {
                let targetname = field(row, "targetname");
                if !targetname.starts_with("staff_craft_") {
                    return None;
                }
                let kind = match targetname.strip_prefix("staff_craft_") {
                    Some("fire") => StaffKind::Fire,
                    Some("ice") => StaffKind::Ice,
                    Some("lightning") => StaffKind::Lightning,
                    Some("gas") => StaffKind::Gas,
                    _ => return None,
                };
                let origin = point(field(row, "origin"))?;
                let angles = point(field(row, "angles")).unwrap_or([0.0; 3]);
                Some(StaffStation {
                    kind,
                    origin,
                    angles,
                    object: None,
                })
            })
            .collect();
    }

    pub(super) fn advance(&mut self, world: &mut World, players: &[(ClientId, [f32; 3])]) {
        for station in &mut self.stations {
            if station.object.is_some()
                || !players.iter().any(|(_, at)| {
                    Vec3::from_array(*at).distance_squared(Vec3::from_array(station.origin))
                        < 1200.0 * 1200.0
                })
            {
                continue;
            }
            if FrameWorld::from_world(world)
                .model_capability(station.kind.model())
                .flatten()
                .is_none()
                || world.resource::<Runtime>().entities.len()
                    >= super::super::entities::MAX_SCRIPT_ENTITIES
            {
                continue;
            }
            let Ok(presence) = super::super::presence::spawn_presence(world, station.origin) else {
                continue;
            };
            let mut runtime = world.resource_mut::<Runtime>();
            let Ok(object) = runtime.create_entity(EntityKind::Spawned, "origins_staff_station")
            else {
                continue;
            };
            runtime.set_object_field(object, "origin", Value::Vector(station.origin));
            runtime.set_object_field(object, "angles", Value::Vector(station.angles));
            runtime.set_object_field(object, "model", Value::string(station.kind.model()));
            let entity = runtime.entities.get_mut(&object).unwrap();
            entity.presence = Some(presence);
            entity.solid = false;
            entity.contents = 0;
            station.object = Some(object);
            diag::info!(
                Sim,
                "origins staff station presented object={object} kind={} origin={:?}",
                station.kind.name(),
                station.origin
            );
        }
    }

    pub(super) fn selected(
        &self,
        world: &mut World,
        client: ClientId,
        origin: [f32; 3],
    ) -> Option<usize> {
        let frame = FrameWorld::from_world(world);
        let forward = Vec3::from_array(math_iw4::angle_vectors(frame.player(client)?.viewangles).0);
        let eye = Vec3::new(origin[0], origin[1], origin[2] + 50.0);
        self.stations
            .iter()
            .enumerate()
            .filter(|(_, station)| {
                station.object.is_some()
                    && Vec3::from_array(origin).distance_squared(Vec3::from_array(station.origin))
                        <= 96.0 * 96.0
                    && (Vec3::from_array(station.origin) + Vec3::Z * 8.0 - eye)
                        .normalize_or_zero()
                        .dot(forward)
                        >= 0.5
                    && frame
                        .trace_world(
                            [origin[0], origin[1], origin[2] + 50.0],
                            [
                                station.origin[0],
                                station.origin[1],
                                station.origin[2] + 8.0,
                            ],
                            [0.0; 3],
                            [0.0; 3],
                            0x11,
                        )
                        .fraction
                        >= 0.95
            })
            .min_by(|(_, a), (_, b)| {
                Vec3::from_array(a.origin)
                    .distance_squared(Vec3::from_array(origin))
                    .total_cmp(
                        &Vec3::from_array(b.origin).distance_squared(Vec3::from_array(origin)),
                    )
            })
            .map(|(index, _)| index)
    }

    pub(super) fn prompt(&self, index: usize, client: ClientId) -> String {
        let Some(station) = self.stations.get(index) else {
            return String::new();
        };
        let has_staff = self.has(client, station.kind);
        let part_count = self.part_count(client, station.kind);
        if has_staff {
            format!("{} staff (owned)", station.kind.name())
        } else if part_count > 0 {
            format!(
                "USE: Craft {} staff ({} part{})",
                station.kind.name(),
                part_count,
                if part_count > 1 { "s" } else { "" }
            )
        } else {
            format!("{} crafting station (need parts)", station.kind.name())
        }
    }

    pub(super) fn craft(&mut self, world: &mut World, client: ClientId, index: usize) -> bool {
        let Some(station) = self.stations.get(index) else {
            return false;
        };
        let kind = station.kind;
        if !self.can_craft(client, kind) {
            return false;
        }
        let parts = self
            .parts
            .entry(client)
            .or_default()
            .entry(kind)
            .or_insert(0);
        *parts = parts.saturating_sub(1);
        self.owned.entry(client).or_default().insert(kind);
        diag::info!(
            Sim,
            "origins staff crafted client={} kind={}",
            client.0,
            kind.name()
        );
        true
    }

    pub(super) fn has(&self, client: ClientId, kind: StaffKind) -> bool {
        self.owned
            .get(&client)
            .is_some_and(|staffs| staffs.contains(&kind))
    }

    pub(super) fn part_count(&self, client: ClientId, kind: StaffKind) -> u8 {
        self.parts
            .get(&client)
            .and_then(|parts| parts.get(&kind))
            .copied()
            .unwrap_or(0)
    }

    pub(super) fn add_part(&mut self, client: ClientId, kind: StaffKind) {
        let count = self
            .parts
            .entry(client)
            .or_default()
            .entry(kind)
            .or_insert(0);
        *count = count.saturating_add(1);
        diag::info!(
            Sim,
            "origins staff part added client={} kind={} count={}",
            client.0,
            kind.name(),
            *count
        );
    }

    pub(super) fn can_craft(&self, client: ClientId, kind: StaffKind) -> bool {
        !self.has(client, kind) && self.part_count(client, kind) >= 1
    }

    pub(super) fn give(&mut self, client: ClientId, kind: StaffKind) {
        self.owned.entry(client).or_default().insert(kind);
        diag::info!(
            Sim,
            "origins staff given client={} kind={}",
            client.0,
            kind.name()
        );
    }

    pub(super) fn take(&mut self, client: ClientId, kind: StaffKind) {
        if let Some(staffs) = self.owned.get_mut(&client) {
            staffs.remove(&kind);
        }
        diag::info!(
            Sim,
            "origins staff taken client={} kind={}",
            client.0,
            kind.name()
        );
    }

    pub(super) fn status(&self, client: ClientId) -> String {
        let staffs = self.owned.get(&client).cloned().unwrap_or_default();
        let parts = self.parts.get(&client).cloned().unwrap_or_default();
        let mut text = "STAFFS:".to_owned();
        for kind in [
            StaffKind::Fire,
            StaffKind::Ice,
            StaffKind::Lightning,
            StaffKind::Gas,
        ] {
            let owned = staffs.contains(&kind);
            let part_count = parts.get(&kind).copied().unwrap_or(0);
            text.push_str(&format!(
                " {}{}{}",
                kind.name(),
                if owned { "+" } else { "-" },
                if part_count > 0 {
                    format!("({})", part_count)
                } else {
                    String::new()
                }
            ));
        }
        text
    }

    pub(super) fn fire_ability(
        &self,
        world: &mut World,
        client: ClientId,
        kind: StaffKind,
        origin: [f32; 3],
        direction: [f32; 3],
        tick: Tick,
    ) -> bool {
        if !self.has(client, kind) {
            return false;
        }
        let effect_name = match kind {
            StaffKind::Fire => "maps/zombie_tomb/fx_tomb_staff_fire",
            StaffKind::Ice => "maps/zombie_tomb/fx_tomb_staff_ice",
            StaffKind::Lightning => "maps/zombie_tomb/fx_tomb_staff_lightning",
            StaffKind::Gas => "maps/zombie_tomb/fx_tomb_staff_gas",
        };
        diag::info!(
            Sim,
            "origins staff ability fired client={} kind={} effect={}",
            client.0,
            kind.name(),
            effect_name
        );
        powerups::effect(world, tick, effect_name, origin);
        true
    }
}

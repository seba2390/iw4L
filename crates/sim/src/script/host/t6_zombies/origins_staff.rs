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
}

impl Staffs {
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
        let count = self.parts.entry(client).or_default().entry(kind).or_insert(0);
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

    pub(super) fn craft(&mut self, world: &mut World, client: ClientId, kind: StaffKind) -> bool {
        if !self.can_craft(client, kind) {
            return false;
        }
        let parts = self.parts.entry(client).or_default().entry(kind).or_insert(0);
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
        for kind in [StaffKind::Fire, StaffKind::Ice, StaffKind::Lightning, StaffKind::Gas] {
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
}

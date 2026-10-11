use super::WeaponRegistry;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WeaponHandle {
    revision: u64,
    row: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WeaponBindingRefusal {
    ForeignOwner,
    UnknownWeapon,
    StaleSnapshot,
}

#[derive(Clone, Copy)]
pub struct BoundWeapon<'a> {
    registry: &'a WeaponRegistry,
    handle: WeaponHandle,
}

impl<'a> BoundWeapon<'a> {
    pub fn movement_scales(self) -> (f32, f32, f32) {
        let facts = &self.registry.rows[self.handle.row as usize].facts;
        (
            facts.move_speed_scale,
            facts.ads_move_speed_scale,
            facts.sprint_duration_scale,
        )
    }

    /// The weapon's own definition bytes, when its game runs its own weapon
    /// rules over them.
    pub fn game_bytes(self) -> Option<&'a std::sync::Arc<super::GameWeaponBytes>> {
        self.registry.rows[self.handle.row as usize]
            .game_bytes
            .as_ref()
    }

    /// The alternate weapon's wire id; 0 when none.
    pub fn alternate_wire_id(self) -> u32 {
        self.registry.rows[self.handle.row as usize].alternate_index
    }

    /// The left-hand weapon's wire id when dual wielding; 0 when none.
    pub fn dual_wield_wire_id(self) -> u32 {
        let row = &self.registry.rows[self.handle.row as usize];
        row.dual_wield_weapon
            .as_deref()
            .and_then(|name| {
                self.registry
                    .by_namespaced
                    .get(&(row.namespace, super::normalize_weapon_name(name)))
            })
            .copied()
            .unwrap_or(0)
    }

    pub fn preparation(self) -> &'a super::WeaponPreparationRecipe {
        &self.registry.rows[self.handle.row as usize].preparation
    }

    pub fn handle(self) -> WeaponHandle {
        self.handle
    }

    pub fn wire_id(self) -> u32 {
        self.handle.row
    }

    pub fn fpv_facts(self) -> Option<super::WeaponFpvFacts> {
        self.registry.fpv_facts_of(self.handle.row)
    }

    pub fn hud_facts(self) -> Option<super::WeaponHudFacts> {
        self.registry.hud_facts_of(self.handle.row)
    }

    pub fn event_facts(self) -> Option<super::WeaponEventFacts> {
        self.registry.event_facts_of(self.handle.row)
    }

    pub fn world_facts(self) -> Option<super::WeaponWorldFacts> {
        self.registry.world_facts_of(self.handle.row)
    }

    pub fn combat_facts(
        self,
        rules: weapon_iw4::WeaponHostRules,
        global_location: Option<[f32; weapon_iw4::HITLOC_COUNT]>,
    ) -> Result<weapon_iw4::WeaponCombatFacts, super::WeaponCombatRefusal> {
        self.registry
            .combat_facts_of(self.handle.row, rules, global_location)
    }
}

impl Default for WeaponRegistry {
    fn default() -> Self {
        crate::WeaponCatalog::default().into_build().registry
    }
}

impl WeaponRegistry {
    pub fn bind(&self, handle: WeaponHandle) -> Result<BoundWeapon<'_>, WeaponBindingRefusal> {
        if handle.revision != self.revision {
            return Err(WeaponBindingRefusal::ForeignOwner);
        }
        if self.rows.get(handle.row as usize).is_none() {
            return Err(WeaponBindingRefusal::UnknownWeapon);
        }
        Ok(BoundWeapon {
            registry: self,
            handle,
        })
    }

    pub fn bind_published_row(&self, row: u32) -> Result<BoundWeapon<'_>, WeaponBindingRefusal> {
        self.bind(WeaponHandle {
            revision: self.revision,
            row,
        })
    }

    pub fn published_weapons(&self) -> impl Iterator<Item = BoundWeapon<'_>> {
        (0..self.rows.len() as u32).map(|row| BoundWeapon {
            registry: self,
            handle: WeaponHandle {
                revision: self.revision,
                row,
            },
        })
    }
}

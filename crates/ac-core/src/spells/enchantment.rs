//! Active enchantments and how they combine: the stacking rule that keeps
//! the strongest spell of each family, the multiplier/additive sum a stat
//! query gets, the death penalty (vitae), and the "in effect" list the
//! effects window shows.

use std::collections::HashMap;

use super::table::SpellTable;
use crate::wire_enum::wire_enum;

/// Bucket of multiplicative enchantments.
pub const BUCKET_MULTIPLICATIVE: u32 = 1;
/// Bucket of additive enchantments.
pub const BUCKET_ADDITIVE: u32 = 2;
/// Bucket of the death penalty (vitae).
pub const BUCKET_VITAE: u32 = 4;
/// Bucket of spell cooldowns.
pub const BUCKET_COOLDOWN: u32 = 8;

/// The stat key of maximum health in the secondary-attribute family.
pub const STAT_KEY_MAX_HEALTH: u32 = 1;
/// The stat key of maximum stamina.
pub const STAT_KEY_MAX_STAMINA: u32 = 3;
/// The stat key of maximum mana.
pub const STAT_KEY_MAX_MANA: u32 = 5;

wire_enum! {
    flags
    /// The stat family an enchantment's stat modifier targets (the low bits
    /// of its stat-mod type).
    EnchantmentTypeFlags {
        ATTRIBUTE = 0x0000_0001 => "Attribute",
        /// A vital maximum (health, stamina, mana).
        SECOND_ATT = 0x0000_0002 => "SecondAtt",
        SKILL = 0x0000_0010 => "Skill",
        /// Applies to every key of the queried family, not one key.
        MULTIPLE_STAT = 0x0000_2000 => "MultipleStat",
        /// A beneficial enchantment (kept by a purge of bad enchantments).
        BENEFICIAL = 0x0200_0000 => "Beneficial",
    }
}

/// One active enchantment as the server described it. Fields the server
/// may leave out are `None`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ActiveEnchantmentRecord {
    /// Spell id.
    pub spell_id: u32,
    /// Layer (several casts of one spell stack as layers).
    pub layer_id: u32,
    /// Duration in seconds; -1 means permanent.
    pub duration: f64,
    /// Who cast it.
    pub caster_guid: u32,
    /// Stat-mod type bits (see [`EnchantmentTypeFlags`]).
    pub stat_mod_type: Option<u32>,
    /// Stat-mod key (which attribute, vital or skill).
    pub stat_mod_key: Option<u32>,
    /// Stat-mod value (a factor or an amount).
    pub stat_mod_value: Option<f32>,
    /// Bucket: 1 multiplicative, 2 additive, 4 vitae, 8 cooldown.
    pub bucket: u32,
    /// Start time on the client's clock (seconds).
    pub start_time: f64,
    /// Spell category (stacking family on the wire).
    pub spell_category: u32,
    /// Power level.
    pub power_level: u32,
    /// Degrade modifier.
    pub degrade_modifier: f32,
    /// Degrade limit.
    pub degrade_limit: f32,
    /// Last time degraded (client clock).
    pub last_time_degraded: f64,
    /// Spell-set id, when the enchantment comes from an item set.
    pub spell_set_id: Option<u32>,
}

impl ActiveEnchantmentRecord {
    /// A record with only its identity, duration and caster.
    #[must_use]
    pub fn new(spell_id: u32, layer_id: u32, duration: f64, caster_guid: u32) -> Self {
        Self {
            spell_id,
            layer_id,
            duration,
            caster_guid,
            ..Self::default()
        }
    }

    /// The packed `(spell, layer)` key the registry stores it under.
    #[must_use]
    pub fn identity(&self) -> u32 {
        make_identity(self.spell_id, self.layer_id)
    }
}

/// Packs a spell id and layer into one key: spell in the low 16 bits,
/// layer in the high 16.
#[must_use]
pub fn make_identity(spell_id: u32, layer_id: u32) -> u32 {
    (spell_id & 0xFFFF) | ((layer_id & 0xFFFF) << 16)
}

/// A stat modifier: `value * multiplier + additive`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VitalMod {
    /// Product of the multiplicative enchantments (and vitae, when included).
    pub multiplier: f32,
    /// Sum of the additive enchantments.
    pub additive: f32,
}

impl VitalMod {
    /// No enchantment applies: `(1.0, 0.0)`.
    pub const IDENTITY: Self = Self {
        multiplier: 1.0,
        additive: 0.0,
    };

    /// A modifier.
    #[must_use]
    pub const fn new(multiplier: f32, additive: f32) -> Self {
        Self {
            multiplier,
            additive,
        }
    }
}

impl Default for VitalMod {
    fn default() -> Self {
        Self::IDENTITY
    }
}

/// The modifier the active enchantments give one stat.
///
/// Within each stacking family (the spell table's family; family 0 stacks
/// per layer instead) only the strongest record counts: the higher power
/// level, then the later start, then the higher spell id. A
/// spell missing from the table is kept, stacking by the record's own spell
/// category (by spell and layer when that is zero). Of those, a record
/// with no value is skipped; vitae multiplies in when `include_vitae`; a
/// multiplicative or additive record counts when its key is `stat_key`
/// (or zero with the multiple-stat bit, when a `required_type` is given)
/// and, with a `required_type`, its type has that bit.
#[must_use]
pub fn get_mod<'a>(
    enchantments: impl IntoIterator<Item = &'a ActiveEnchantmentRecord>,
    table: &SpellTable,
    stat_key: u32,
    required_type: Option<EnchantmentTypeFlags>,
    include_vitae: bool,
) -> VitalMod {
    // Strongest record per family, in first-seen order.
    let mut slot_by_family: HashMap<u64, usize> = HashMap::new();
    let mut stronger: Vec<&ActiveEnchantmentRecord> = Vec::new();
    for enchantment in enchantments {
        // A spell the local table lacks (a server with its own spells) still
        // counts, with the server's value, stacking by the record's category.
        let family = match table.get(enchantment.spell_id) {
            Some(meta) if meta.family != 0 => u64::from(meta.family),
            Some(_) => u64::from(enchantment.layer_id | 0x8000_0000),
            None if enchantment.spell_category != 0 => {
                (1 << 32) | u64::from(enchantment.spell_category)
            }
            None => {
                (2 << 32) | u64::from(make_identity(enchantment.spell_id, enchantment.layer_id))
            }
        };
        match slot_by_family.get(&family) {
            Some(&slot) => {
                if outranks(enchantment, stronger[slot]) {
                    stronger[slot] = enchantment;
                }
            }
            None => {
                slot_by_family.insert(family, stronger.len());
                stronger.push(enchantment);
            }
        }
    }

    let mut multiplier = 1.0f32;
    let mut additive = 0.0f32;
    let mut vitae = 1.0f32;
    for enchantment in stronger {
        let Some(value) = enchantment.stat_mod_value else {
            continue;
        };
        if enchantment.bucket == BUCKET_VITAE {
            if include_vitae {
                vitae *= value;
            }
            continue;
        }
        let Some(key) = enchantment.stat_mod_key else {
            continue;
        };
        let mod_type = enchantment.stat_mod_type.unwrap_or(0);
        let all_stat = key == 0
            && required_type.is_some()
            && mod_type & EnchantmentTypeFlags::MULTIPLE_STAT.bits() != 0;
        if !all_stat && key != stat_key {
            continue;
        }
        if let Some(required) = required_type
            && mod_type & required.bits() == 0
        {
            continue;
        }
        match enchantment.bucket {
            BUCKET_MULTIPLICATIVE => multiplier *= value,
            BUCKET_ADDITIVE => additive += value,
            _ => {}
        }
    }
    multiplier *= vitae;
    if multiplier == 1.0 && additive == 0.0 {
        VitalMod::IDENTITY
    } else {
        VitalMod::new(multiplier, additive)
    }
}

/// Whether `candidate` replaces `incumbent` in a stacking family: the higher
/// power level wins, then the later start, then the higher spell id. Spell
/// ids do not rank strength: on a server that boosts its buffs, a timed buff
/// with a lower id can carry the higher power level and value, and the totals
/// the server reports count that one.
fn outranks(candidate: &ActiveEnchantmentRecord, incumbent: &ActiveEnchantmentRecord) -> bool {
    let (candidate_power, incumbent_power) = (
        candidate.power_level.cast_signed(),
        incumbent.power_level.cast_signed(),
    );
    candidate_power > incumbent_power
        || (candidate_power == incumbent_power
            && (candidate.start_time > incumbent.start_time
                || (candidate.start_time == incumbent.start_time
                    && candidate.spell_id > incumbent.spell_id)))
}

/// The modifier a skill gets: [`get_mod`] restricted to skill records,
/// vitae included.
#[must_use]
pub fn get_skill_mod<'a>(
    enchantments: impl IntoIterator<Item = &'a ActiveEnchantmentRecord>,
    table: &SpellTable,
    skill_id: u32,
) -> VitalMod {
    get_mod(
        enchantments,
        table,
        skill_id,
        Some(EnchantmentTypeFlags::SKILL),
        true,
    )
}

/// An attribute after enchantments: `base * multiplier + additive`,
/// floored at 1 for a base below 10 and at 10 otherwise, then truncated.
#[must_use]
pub fn enchant_attribute(modifier: VitalMod, base_value: u32) -> i32 {
    let mut value = base_value as f32 * modifier.multiplier + modifier.additive;
    let floor = if base_value < 10 { 1.0 } else { 10.0 };
    if value < floor {
        value = floor;
    }
    value as i32
}

/// A skill after enchantments: `base * multiplier + additive`, zero below
/// one half, then truncated.
#[must_use]
pub fn enchant_skill(modifier: VitalMod, base_value: u32) -> i32 {
    let mut value = base_value as f32 * modifier.multiplier + modifier.additive;
    if value < 0.5 {
        value = 0.0;
    }
    value as i32
}

/// The product of every vitae record's value (1 when there is none).
#[must_use]
pub fn get_vitae_multiplier<'a>(
    enchantments: impl IntoIterator<Item = &'a ActiveEnchantmentRecord>,
) -> f32 {
    enchantments
        .into_iter()
        .filter(|e| e.bucket == BUCKET_VITAE)
        .filter_map(|e| e.stat_mod_value)
        .fold(1.0, |vitae, value| vitae * value)
}

/// How much vitae takes off a value: `trunc(base * vitae) - base`, zero
/// with no vitae.
#[must_use]
pub fn vitae_modifier(vitae_multiplier: f32, base_value: u32) -> i32 {
    if vitae_multiplier == 1.0 {
        return 0;
    }
    // The game works in 32-bit signed integers here; a base above
    // `i32::MAX` wraps as it does there.
    ((base_value as f32 * vitae_multiplier) as i32).wrapping_sub(base_value.cast_signed())
}

/// [`vitae_modifier`] with the vitae taken from the active enchantments.
#[must_use]
pub fn skill_vitae_modifier<'a>(
    enchantments: impl IntoIterator<Item = &'a ActiveEnchantmentRecord>,
    base_value: u32,
) -> i32 {
    vitae_modifier(get_vitae_multiplier(enchantments), base_value)
}

/// The enchantments in effect, as the effects window lists them: the
/// multiplicative list, then the additive list, each walked in order. Two
/// records of one spell category duel: the higher power level (compared as
/// signed) wins, then the later start time; on a full tie the incumbent
/// stays. A winning challenger replaces the incumbent and moves to the end.
#[must_use]
pub fn get_enchantments_in_effect(
    enchantments: &[ActiveEnchantmentRecord],
) -> Vec<ActiveEnchantmentRecord> {
    let mut result = Vec::new();
    duel_list(enchantments, BUCKET_MULTIPLICATIVE, &mut result);
    duel_list(enchantments, BUCKET_ADDITIVE, &mut result);
    result
}

fn duel_list(
    source: &[ActiveEnchantmentRecord],
    bucket: u32,
    result: &mut Vec<ActiveEnchantmentRecord>,
) {
    for candidate in source.iter().filter(|c| c.bucket == bucket) {
        if let Some(index) = result
            .iter()
            .position(|existing| existing.spell_category == candidate.spell_category)
        {
            if !candidate_wins(&result[index], candidate) {
                continue;
            }
            result.remove(index);
        }
        result.push(*candidate);
    }
}

fn candidate_wins(
    incumbent: &ActiveEnchantmentRecord,
    candidate: &ActiveEnchantmentRecord,
) -> bool {
    let incumbent_power = incumbent.power_level.cast_signed();
    let candidate_power = candidate.power_level.cast_signed();
    candidate_power > incumbent_power
        || (candidate_power == incumbent_power && candidate.start_time > incumbent.start_time)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spells::SpellMetadata;

    // Golden values from OpenAC
    // `tests/AcDream.Core.Tests/Spells/EnchantmentMathTests.cs` and
    // `EnchantmentRegistryProjectionTests.cs`.

    fn table(rows: &[(u32, u32)]) -> SpellTable {
        SpellTable::create(
            rows.iter()
                .map(|&(id, family)| SpellMetadata::new(id, "Test", family)),
        )
        .expect("unique ids")
    }

    fn record(
        spell_id: u32,
        layer: u32,
        bucket: u32,
        stat_mod_type: u32,
        key: u32,
        value: f32,
    ) -> ActiveEnchantmentRecord {
        ActiveEnchantmentRecord {
            stat_mod_type: Some(stat_mod_type),
            stat_mod_key: Some(key),
            stat_mod_value: Some(value),
            bucket,
            ..ActiveEnchantmentRecord::new(spell_id, layer, 60.0, 0)
        }
    }

    fn mult(spell_id: u32, layer: u32, key: u32, value: f32) -> ActiveEnchantmentRecord {
        record(spell_id, layer, 1, 0, key, value)
    }

    fn add(spell_id: u32, layer: u32, key: u32, value: f32) -> ActiveEnchantmentRecord {
        record(spell_id, layer, 2, 0, key, value)
    }

    fn vitae(spell_id: u32, layer: u32, key: u32, value: f32) -> ActiveEnchantmentRecord {
        ActiveEnchantmentRecord {
            duration: -1.0,
            ..record(spell_id, layer, 4, 0, key, value)
        }
    }

    fn vital_mod(list: &[ActiveEnchantmentRecord], table: &SpellTable, key: u32) -> VitalMod {
        get_mod(list, table, key, None, true)
    }

    fn close(actual: f32, expected: f32) -> bool {
        (actual - expected).abs() < 1e-4
    }

    #[test]
    fn empty_is_identity() {
        assert_eq!(
            vital_mod(&[], &SpellTable::empty(), STAT_KEY_MAX_STAMINA),
            VitalMod::IDENTITY
        );
    }

    #[test]
    fn valueless_records_missing_from_the_table_contribute_nothing() {
        let list = [
            ActiveEnchantmentRecord::new(9999, 1, 60.0, 0),
            ActiveEnchantmentRecord::new(8888, 2, 60.0, 0),
        ];
        assert_eq!(
            vital_mod(&list, &SpellTable::empty(), STAT_KEY_MAX_STAMINA),
            VitalMod::IDENTITY
        );
    }

    #[test]
    fn spells_missing_from_the_table_use_the_servers_value() {
        let list = [add(9999, 1, 5, 56.0)];
        let m = vital_mod(&list, &SpellTable::empty(), 5);
        assert!(m.additive == 56.0);
    }

    #[test]
    fn unknown_spells_stack_by_the_records_category() {
        let mut weak = add(9000, 1, 5, 40.0);
        let mut strong = add(9001, 2, 5, 56.0);
        weak.spell_category = 7;
        strong.spell_category = 7;
        let m = vital_mod(&[weak, strong], &SpellTable::empty(), 5);
        assert!(m.additive == 56.0);
        // Without a category, distinct spells stack.
        let m = vital_mod(
            &[add(9000, 1, 5, 40.0), add(9001, 2, 5, 56.0)],
            &SpellTable::empty(),
            5,
        );
        assert!(m.additive == 96.0);
    }

    #[test]
    fn a_higher_power_level_beats_a_higher_spell_id_in_one_family() {
        let t = table(&[(2245, 37), (4560, 37)]);
        let mut timed = add(2245, 2, 5, 56.0);
        let mut permanent = add(4560, 1, 5, 45.0);
        timed.power_level = 16300;
        permanent.power_level = 400;
        assert!(vital_mod(&[timed, permanent], &t, 5).additive == 56.0);
        // Equal power falls back to the higher spell id.
        timed.power_level = 400;
        assert!(vital_mod(&[timed, permanent], &t, 5).additive == 45.0);
    }

    #[test]
    fn equal_power_goes_to_the_later_start() {
        let t = table(&[(2245, 37), (4560, 37)]);
        let mut earlier = add(4560, 1, 5, 45.0);
        let mut later = add(2245, 2, 5, 56.0);
        earlier.power_level = 400;
        later.power_level = 400;
        earlier.start_time = 10.0;
        later.start_time = 20.0;
        assert!(vital_mod(&[earlier, later], &t, 5).additive == 56.0);
    }

    #[test]
    fn stat_key_constants() {
        assert_eq!(STAT_KEY_MAX_HEALTH, 1);
        assert_eq!(STAT_KEY_MAX_STAMINA, 3);
        assert_eq!(STAT_KEY_MAX_MANA, 5);
        assert_eq!(EnchantmentTypeFlags::SKILL.bits(), 0x10);
        assert_eq!(EnchantmentTypeFlags::SECOND_ATT.bits(), 0x2);
    }

    #[test]
    fn records_without_values_contribute_nothing() {
        let t = table(&[(1, 1), (132, 1), (10, 0), (20, 0)]);
        let list = [
            ActiveEnchantmentRecord::new(1, 100, 60.0, 0),
            ActiveEnchantmentRecord::new(132, 101, 60.0, 0),
            ActiveEnchantmentRecord::new(10, 100, 60.0, 0),
            ActiveEnchantmentRecord::new(20, 101, 60.0, 0),
        ];
        assert_eq!(
            vital_mod(&list, &t, STAT_KEY_MAX_STAMINA),
            VitalMod::IDENTITY
        );
    }

    #[test]
    fn multiplicative_bucket_multiplies() {
        let t = table(&[(10, 100), (11, 200)]);
        let m = vital_mod(&[mult(10, 1, 3, 1.2), mult(11, 2, 3, 1.1)], &t, 3);
        assert!(close(m.multiplier, 1.32));
        assert!(m.additive == 0.0);
    }

    #[test]
    fn additive_bucket_sums() {
        let t = table(&[(20, 300), (21, 301)]);
        let m = vital_mod(&[add(20, 1, 5, 25.0), add(21, 2, 5, 50.0)], &t, 5);
        assert!(m.multiplier == 1.0);
        assert!(m.additive == 75.0);
    }

    #[test]
    fn stat_key_mismatch_contributes_nothing() {
        let t = table(&[(30, 500)]);
        assert_eq!(vital_mod(&[mult(30, 1, 1, 1.5)], &t, 3), VitalMod::IDENTITY);
    }

    #[test]
    fn vitae_multiplies_after_buffs() {
        let t = table(&[(40, 0), (41, 0)]);
        let m = vital_mod(&[add(40, 1, 1, 10.0), vitae(41, 2, 1, 0.85)], &t, 1);
        assert!(close(m.multiplier, 0.85));
        assert!(m.additive == 10.0);
    }

    #[test]
    fn vitae_applies_with_key_zero() {
        let t = table(&[(666, 0)]);
        let m = vital_mod(&[vitae(666, 0, 0, 0.95)], &t, STAT_KEY_MAX_STAMINA);
        assert!(close(m.multiplier, 0.95));
    }

    #[test]
    fn family_stacking_picks_higher_spell_id() {
        let t = table(&[(10, 1), (132, 1)]);
        let m = vital_mod(&[mult(10, 1, 3, 1.1), mult(132, 2, 3, 1.5)], &t, 3);
        assert!(close(m.multiplier, 1.5));
    }

    #[test]
    fn required_type_excludes_cross_domain_key_collision() {
        let t = table(&[(50, 0)]);
        let list = [record(
            50,
            1,
            1,
            EnchantmentTypeFlags::ATTRIBUTE.bits(),
            1,
            1.5,
        )];
        assert_eq!(
            get_mod(&list, &t, 1, Some(EnchantmentTypeFlags::SECOND_ATT), true),
            VitalMod::IDENTITY
        );
        let m = get_mod(&list, &t, 1, Some(EnchantmentTypeFlags::ATTRIBUTE), true);
        assert!(close(m.multiplier, 1.5));
    }

    #[test]
    fn multiple_stat_key_zero_applies_to_every_attribute() {
        let t = table(&[(70, 900), (71, 901)]);
        let blessing =
            (EnchantmentTypeFlags::ATTRIBUTE | EnchantmentTypeFlags::MULTIPLE_STAT).bits();
        let list = [
            record(70, 1, 2, EnchantmentTypeFlags::ATTRIBUTE.bits(), 1, 40.0),
            record(71, 2, 2, blessing, 0, 9.0),
        ];
        for key in 1..=6 {
            let m = get_mod(&list, &t, key, Some(EnchantmentTypeFlags::ATTRIBUTE), true);
            assert!(m.additive == if key == 1 { 49.0 } else { 9.0 }, "key {key}");
        }
    }

    #[test]
    fn key_zero_without_multiple_stat_contributes_nothing() {
        let t = table(&[(72, 902)]);
        let list = [record(
            72,
            1,
            2,
            EnchantmentTypeFlags::ATTRIBUTE.bits(),
            0,
            9.0,
        )];
        assert_eq!(
            get_mod(&list, &t, 1, Some(EnchantmentTypeFlags::ATTRIBUTE), true),
            VitalMod::IDENTITY
        );
    }

    #[test]
    fn multiple_stat_attribute_does_not_leak_into_vitals_or_skills() {
        let t = table(&[(73, 903)]);
        let blessing =
            (EnchantmentTypeFlags::ATTRIBUTE | EnchantmentTypeFlags::MULTIPLE_STAT).bits();
        let list = [record(73, 1, 2, blessing, 0, 9.0)];
        assert_eq!(
            get_mod(&list, &t, 1, Some(EnchantmentTypeFlags::SECOND_ATT), true),
            VitalMod::IDENTITY
        );
        assert_eq!(get_skill_mod(&list, &t, 1), VitalMod::IDENTITY);
    }

    #[test]
    fn key_zero_without_required_type_contributes_nothing() {
        let t = table(&[(74, 904)]);
        let blessing =
            (EnchantmentTypeFlags::ATTRIBUTE | EnchantmentTypeFlags::MULTIPLE_STAT).bits();
        let list = [record(74, 1, 2, blessing, 0, 9.0)];
        assert_eq!(get_mod(&list, &t, 1, None, true), VitalMod::IDENTITY);
    }

    #[test]
    fn include_vitae_false_excludes_vitae() {
        let t = table(&[(60, 0)]);
        assert_eq!(
            get_mod(
                &[vitae(60, 1, 0, 0.67)],
                &t,
                1,
                Some(EnchantmentTypeFlags::ATTRIBUTE),
                false
            ),
            VitalMod::IDENTITY
        );
    }

    #[test]
    fn enchant_attribute_rules() {
        assert_eq!(enchant_attribute(VitalMod::IDENTITY, 200), 200);
        assert_eq!(enchant_attribute(VitalMod::new(1.1, 0.0), 200), 220);
        assert_eq!(enchant_attribute(VitalMod::new(0.1, 0.0), 5), 1);
        assert_eq!(enchant_attribute(VitalMod::new(0.1, 0.0), 50), 10);
    }

    #[test]
    fn enchant_skill_rules() {
        // 33% vitae on a base-303 skill: 303 * 0.67 = 203.01 -> 203.
        assert_eq!(enchant_skill(VitalMod::new(0.67, 0.0), 303), 203);
        assert_eq!(enchant_skill(VitalMod::new(0.67, 50.0), 300), 251);
        assert_eq!(enchant_skill(VitalMod::new(0.001, 0.0), 10), 0);
    }

    #[test]
    fn vitae_multiplier_and_modifier() {
        assert!(get_vitae_multiplier(&[mult(1, 1, 1, 1.5)]) == 1.0);
        let list = [vitae(1, 1, 0, 0.67), mult(2, 2, 1, 1.5)];
        assert!(close(get_vitae_multiplier(&list), 0.67));
        assert_eq!(skill_vitae_modifier(&[vitae(1, 1, 0, 0.67)], 303), -100);
        assert_eq!(skill_vitae_modifier(&[], 303), 0);
    }

    #[test]
    fn skill_mod_rules() {
        let t = table(&[(50, 400), (51, 401), (52, 402), (53, 403), (54, 0)]);
        let skill = EnchantmentTypeFlags::SKILL.bits();
        assert_eq!(
            get_skill_mod(&[], &SpellTable::empty(), 24),
            VitalMod::IDENTITY
        );
        assert!(close(
            get_skill_mod(&[record(50, 1, 1, skill, 24, 1.2)], &t, 24).multiplier,
            1.2
        ));
        assert!(close(
            get_skill_mod(&[record(51, 1, 2, skill, 22, 15.0)], &t, 22).additive,
            15.0
        ));
        assert_eq!(
            get_skill_mod(&[record(52, 1, 1, skill, 7, 1.5)], &t, 24),
            VitalMod::IDENTITY
        );
        let vital_typed = [record(
            53,
            1,
            1,
            EnchantmentTypeFlags::SECOND_ATT.bits(),
            24,
            1.5,
        )];
        assert_eq!(get_skill_mod(&vital_typed, &t, 24), VitalMod::IDENTITY);
        assert!(close(
            get_mod(&vital_typed, &t, 24, None, true).multiplier,
            1.5
        ));
        assert!(close(
            get_skill_mod(&[vitae(54, 0, 0, 0.9)], &t, 24).multiplier,
            0.9
        ));
    }

    fn effect(
        spell_id: u32,
        bucket: u32,
        category: u32,
        power: u32,
        start: f64,
    ) -> ActiveEnchantmentRecord {
        ActiveEnchantmentRecord {
            bucket,
            start_time: start,
            spell_category: category,
            power_level: power,
            ..ActiveEnchantmentRecord::new(spell_id, spell_id, 60.0, 1)
        }
    }

    fn ids(list: &[ActiveEnchantmentRecord]) -> Vec<u32> {
        list.iter().map(|r| r.spell_id).collect()
    }

    #[test]
    fn in_effect_excludes_vitae_and_cooldown() {
        let result = get_enchantments_in_effect(&[
            effect(1, 1, 1, 1, 0.0),
            effect(2, 4, 2, 1, 0.0),
            effect(3, 8, 3, 1, 0.0),
            effect(4, 2, 4, 1, 0.0),
        ]);
        assert_eq!(ids(&result), [1, 4]);
    }

    #[test]
    fn in_effect_duel_prefers_higher_power() {
        let result =
            get_enchantments_in_effect(&[effect(1, 1, 12, 1, 0.0), effect(2, 1, 12, 7, 0.0)]);
        assert_eq!(ids(&result), [2]);
    }

    #[test]
    fn in_effect_power_compares_signed() {
        let result = get_enchantments_in_effect(&[
            effect(1, 1, 12, 1, 0.0),
            effect(2, 1, 12, u32::MAX, 0.0),
        ]);
        assert_eq!(ids(&result), [1]);
    }

    #[test]
    fn in_effect_equal_power_prefers_later_start_and_keeps_exact_tie() {
        let result = get_enchantments_in_effect(&[
            effect(1, 1, 12, 7, 10.0),
            effect(2, 1, 12, 7, 20.0),
            effect(3, 2, 12, 7, 20.0),
        ]);
        assert_eq!(ids(&result), [2]);
    }

    #[test]
    fn in_effect_replacing_winner_appends() {
        let result = get_enchantments_in_effect(&[
            effect(1, 1, 10, 1, 0.0),
            effect(2, 1, 20, 1, 0.0),
            effect(3, 2, 10, 2, 0.0),
        ]);
        assert_eq!(ids(&result), [2, 3]);
    }
}

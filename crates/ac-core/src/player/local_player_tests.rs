//! Golden behavior from OpenAC
//! `tests/AcDream.Core.Tests/Player/LocalPlayerStateTests.cs` and
//! `SkillLevelConformanceTests.cs`.

use std::collections::BTreeMap;
use std::sync::Arc;

use super::*;
use crate::spells::{ActiveEnchantmentRecord, EnchantmentTypeFlags, SpellMetadata, SpellTable};

fn book(ids: &[u32]) -> Spellbook {
    Spellbook::new(Some(Arc::new(
        SpellTable::create(ids.iter().map(|&id| SpellMetadata::new(id, "Test", 0)))
            .expect("unique"),
    )))
}

fn typed(
    spell_id: u32,
    layer: u32,
    stat_mod_type: u32,
    key: u32,
    value: f32,
    bucket: u32,
) -> ActiveEnchantmentRecord {
    ActiveEnchantmentRecord {
        stat_mod_type: Some(stat_mod_type),
        stat_mod_key: Some(key),
        stat_mod_value: Some(value),
        bucket,
        ..ActiveEnchantmentRecord::new(spell_id, layer, 60.0, 0)
    }
}

fn vitae(spell_id: u32, value: f32) -> ActiveEnchantmentRecord {
    ActiveEnchantmentRecord {
        duration: -1.0,
        ..typed(spell_id, 1, 0, 0, value, 4)
    }
}

const ATTRIBUTE: u32 = EnchantmentTypeFlags::ATTRIBUTE.0;
const SECOND_ATT: u32 = EnchantmentTypeFlags::SECOND_ATT.0;
const SKILL: u32 = EnchantmentTypeFlags::SKILL.0;

fn close(actual: f32, expected: f32) -> bool {
    (actual - expected).abs() < 1e-3
}

#[test]
fn defaults_are_unknown() {
    let s = LocalPlayerState::default();
    for kind in VitalKind::ALL {
        assert_eq!(s.get(kind), None);
    }
    assert_eq!(s.health_percent(), None);
    assert_eq!(s.stamina_percent(), None);
    assert_eq!(s.mana_percent(), None);
}

#[test]
fn vital_ids_map_both_id_systems() {
    for (id, kind) in [
        (1, VitalKind::Health),
        (2, VitalKind::Health),
        (3, VitalKind::Stamina),
        (4, VitalKind::Stamina),
        (5, VitalKind::Mana),
        (6, VitalKind::Mana),
        (7, VitalKind::Health),
        (8, VitalKind::Stamina),
        (9, VitalKind::Mana),
    ] {
        assert_eq!(vital_id_to_kind(id), Some(kind));
    }
    for id in [0, 10, 99] {
        assert_eq!(vital_id_to_kind(id), None);
    }
}

#[test]
fn attribute_ids_map_primary_attributes() {
    for (id, kind) in [
        (1, AttributeKind::Strength),
        (2, AttributeKind::Endurance),
        (3, AttributeKind::Quickness),
        (4, AttributeKind::Coordination),
        (5, AttributeKind::Focus),
        (6, AttributeKind::Self_),
    ] {
        assert_eq!(attribute_id_to_kind(id), Some(kind));
        assert_eq!(kind.id(), id);
    }
    for id in [0, 7, 99] {
        assert_eq!(attribute_id_to_kind(id), None);
    }
}

#[test]
fn vital_update_populates_snapshot() {
    let mut s = LocalPlayerState::default();
    assert_eq!(
        s.on_vital_update(4, 100, 120, 50_000, 180),
        Some(VitalKind::Stamina)
    );
    assert_eq!(
        s.get(VitalKind::Stamina),
        Some(VitalSnapshot {
            ranks: 100,
            start: 120,
            xp: 50_000,
            current: 180
        })
    );
}

#[test]
fn attribute_update_populates_snapshot() {
    let mut s = LocalPlayerState::default();
    s.on_attribute_update(2, 50, 100, 12_345);
    let endurance = s.get_attribute(AttributeKind::Endurance).expect("received");
    assert_eq!(endurance.ranks, 50);
    assert_eq!(endurance.start, 100);
    assert_eq!(endurance.xp, 12_345);
    assert_eq!(endurance.current(), 150);
}

#[test]
fn health_uses_half_endurance() {
    let mut s = LocalPlayerState::default();
    s.on_attribute_update(2, 50, 150, 0);
    s.on_vital_update(7, 0, 0, 0, 80);
    assert_eq!(s.get_max_approx(VitalKind::Health), Some(100));
    assert!(close(s.health_percent().expect("known"), 0.8));
}

#[test]
fn stamina_uses_full_endurance() {
    let mut s = LocalPlayerState::default();
    s.on_attribute_update(2, 50, 150, 0);
    s.on_vital_update(8, 0, 0, 0, 150);
    assert_eq!(s.get_max_approx(VitalKind::Stamina), Some(200));
    assert!(close(s.stamina_percent().expect("known"), 0.75));
}

#[test]
fn mana_uses_self() {
    let mut s = LocalPlayerState::default();
    s.on_attribute_update(6, 50, 50, 0);
    s.on_vital_update(9, 20, 80, 0, 100);
    assert_eq!(s.get_max_approx(VitalKind::Mana), Some(200));
    assert!(close(s.mana_percent().expect("known"), 0.5));
}

#[test]
fn zero_maximum_has_no_percent() {
    let mut s = LocalPlayerState::default();
    s.on_vital_update(8, 0, 0, 0, 0);
    assert_eq!(s.stamina_percent(), None);
}

#[test]
fn percent_clamps_to_one() {
    let mut s = LocalPlayerState::default();
    s.on_attribute_update(2, 50, 50, 0);
    s.on_vital_update(8, 0, 0, 0, 150);
    assert_eq!(s.stamina_percent(), Some(1.0));
}

#[test]
fn attribute_modifier_with_colliding_key_does_not_affect_health() {
    let mut b = book(&[1]);
    b.on_enchantment_added(typed(1, 1, ATTRIBUTE, 1, 0.99999, 1));
    let mut s = LocalPlayerState::new(Some(b));
    s.on_vital_update(7, 99_999, 0, 0, 99_999);
    assert_eq!(s.get_base_max_approx(VitalKind::Health), Some(99_999));
    assert_eq!(s.get_max_approx(VitalKind::Health), Some(99_999));
    assert_eq!(s.health_percent(), Some(1.0));
}

// GetMaxApprox_SecondaryAttributeModifierRoundsToNearest (OpenAC v0.1.25,
// #211: 100.75 shows as 101, no longer truncated to 100).
#[test]
fn secondary_attribute_modifier_rounds_to_nearest() {
    let mut b = book(&[1]);
    b.on_enchantment_added(typed(1, 1, SECOND_ATT, STAT_KEY_MAX_HEALTH, 0.75, 2));
    let mut s = LocalPlayerState::new(Some(b));
    s.on_vital_update(7, 100, 0, 0, 100);
    assert_eq!(s.get_max_approx(VitalKind::Health), Some(101));
}

// VitalMaximumAndVitaePenaltyRoundBeforePresentation (OpenAC v0.1.25):
// base 180, vitae 0.97, +23 from a vital spell.
#[test]
fn vital_maximum_and_vitae_penalty_round_before_presentation() {
    for (key, kind) in [
        (1, VitalKind::Health),
        (3, VitalKind::Stamina),
        (5, VitalKind::Mana),
    ] {
        let mut b = book(&[1, 2]);
        b.on_enchantment_added(vitae(1, 0.97));
        b.on_enchantment_added(typed(2, 2, SECOND_ATT, key, 23.0, 2));
        let mut s = LocalPlayerState::new(Some(b));
        s.on_vital_update(key, 180, 0, 0, 198);
        assert_eq!(s.get_max_approx(kind), Some(198), "{kind:?}");
        assert_eq!(s.get_vital_vitae_modifier(kind), -5, "{kind:?}");
        assert_eq!(s.get_base_max_approx(kind), Some(180), "{kind:?}");
    }
}

// VitalRoundingPreservesMinimumAndHalfUpBoundary (OpenAC v0.1.25):
// (base, vitae, maximum, vitae delta).
#[test]
fn vital_rounding_keeps_the_minimum_and_the_half_up_boundary() {
    for (base, multiplier, maximum, delta) in [
        (1, 0.1, 1, -1),
        (5, 0.6, 5, -2),
        (100, 0.975, 98, -2),
        (100, 0.974, 97, -3),
        (100, 1.0, 100, 0),
    ] {
        let mut b = book(&[1]);
        b.on_enchantment_added(vitae(1, multiplier));
        let mut s = LocalPlayerState::new(Some(b));
        s.on_vital_update(1, base, 0, 0, 0);
        assert_eq!(
            s.get_max_approx(VitalKind::Health),
            Some(maximum),
            "{base} x {multiplier}"
        );
        assert_eq!(
            s.get_vital_vitae_modifier(VitalKind::Health),
            delta,
            "{base} x {multiplier}"
        );
    }
}

#[test]
fn attribute_buffs_feed_vital_formula_exact_live_regression() {
    let mut b = book(&[1, 2]);
    b.on_enchantment_added(typed(1, 1, ATTRIBUTE, 2, 15.0, 2)); // Endurance
    b.on_enchantment_added(typed(2, 2, ATTRIBUTE, 6, 15.0, 2)); // Self
    let mut s = LocalPlayerState::new(Some(b));
    s.on_attribute_update(2, 0, 30, 0);
    s.on_attribute_update(6, 0, 10, 0);
    s.on_vital_update(7, 0, 15, 0, 38);
    s.on_vital_update(8, 0, 30, 0, 75);
    s.on_vital_update(9, 0, 0, 0, 25);

    assert_eq!(s.get_base_max_approx(VitalKind::Health), Some(30));
    assert_eq!(s.get_base_max_approx(VitalKind::Stamina), Some(60));
    assert_eq!(s.get_base_max_approx(VitalKind::Mana), Some(10));
    assert_eq!(s.get_max_approx(VitalKind::Health), Some(38));
    assert_eq!(s.get_max_approx(VitalKind::Stamina), Some(75));
    assert_eq!(s.get_max_approx(VitalKind::Mana), Some(25));
    assert_eq!(s.health_percent(), Some(1.0));
    assert_eq!(s.stamina_percent(), Some(1.0));
    assert_eq!(s.mana_percent(), Some(1.0));
}

#[test]
fn health_rounds_half_endurance_and_includes_gear() {
    let mut s = LocalPlayerState::default();
    s.on_int_properties(BTreeMap::from([(PropertyInt::GEAR_MAX_HEALTH.0, 7)]));
    s.on_attribute_update(2, 0, 45, 0);
    s.on_vital_update(7, 0, 10, 0, 40);
    assert_eq!(s.get_base_max_approx(VitalKind::Health), Some(40));
    assert_eq!(s.get_max_approx(VitalKind::Health), Some(40));
}

#[test]
fn vital_current_updates_only_current() {
    let mut s = LocalPlayerState::default();
    s.on_attribute_update(2, 50, 150, 0);
    s.on_vital_update(8, 0, 0, 50_000, 180);
    assert_eq!(s.on_vital_current(8, 90), Some(VitalKind::Stamina));
    let stamina = s.get(VitalKind::Stamina).expect("received");
    assert_eq!(stamina.ranks, 0);
    assert_eq!(stamina.start, 0);
    assert_eq!(stamina.xp, 50_000);
    assert_eq!(stamina.current, 90);
    assert!(close(s.stamina_percent().expect("known"), 0.45));
}

#[test]
fn vital_current_before_full_record_is_ignored() {
    let mut s = LocalPlayerState::default();
    assert_eq!(s.on_vital_current(4, 90), None);
    assert_eq!(s.get(VitalKind::Stamina), None);
    assert_eq!(s.stamina_percent(), None);
}

#[test]
fn attribute_update_names_derived_vitals() {
    let mut s = LocalPlayerState::default();
    let endurance = s.on_attribute_update(2, 10, 100, 500).expect("known id");
    assert_eq!(endurance.kind, AttributeKind::Endurance);
    assert_eq!(
        endurance.affected_vitals,
        [VitalKind::Health, VitalKind::Stamina]
    );
    let self_ = s.on_attribute_update(6, 5, 100, 250).expect("known id");
    assert_eq!(self_.affected_vitals, [VitalKind::Mana]);
    let quickness = s.on_attribute_update(3, 1, 100, 10).expect("known id");
    assert!(quickness.affected_vitals.is_empty());
    assert_eq!(s.on_attribute_update(7, 1, 1, 1), None);
}

#[test]
fn attribute_update_refreshes_percent() {
    let mut s = LocalPlayerState::default();
    s.on_vital_update(8, 0, 0, 0, 100);
    assert_eq!(s.stamina_percent(), None);
    s.on_attribute_update(2, 50, 150, 0);
    assert!(close(s.stamina_percent().expect("known"), 0.5));
}

#[test]
fn skill_update_stores_formula_adjusted_level() {
    let mut s = LocalPlayerState::default();
    s.on_skill_update(24, 12, 2, 3456, 30, 0, 1.5, 80);
    let run = s.get_skill(24).expect("received");
    assert_eq!(run.current_level(), 122);
    assert_eq!(run.status, 2);
    assert_eq!(run.xp, 3456);
}

#[test]
fn clear_returns_to_pre_login() {
    let mut s = LocalPlayerState::default();
    s.on_vital_update(7, 1, 2, 3, 4);
    s.on_attribute_update(1, 5, 6, 7);
    s.on_skill_update(8, 9, 2, 10, 11, 12, 13.0, 14);
    s.on_int_properties(BTreeMap::from([(1, 42)]));
    s.clear();
    assert_eq!(s.get(VitalKind::Health), None);
    assert_eq!(s.get_attribute(AttributeKind::Strength), None);
    assert!(s.skills().is_empty());
    assert!(s.int_properties().is_empty());
    s.clear();
    assert!(s.skills().is_empty());
}

#[test]
fn effective_attribute_without_spellbook_is_base() {
    let mut s = LocalPlayerState::default();
    s.on_attribute_update(1, 100, 100, 0);
    assert_eq!(
        s.get_effective_attribute(AttributeKind::Strength),
        Some(200)
    );
}

#[test]
fn effective_attribute_unseen_is_none() {
    let s = LocalPlayerState::new(Some(Spellbook::default()));
    assert_eq!(s.get_effective_attribute(AttributeKind::Strength), None);
}

#[test]
fn attributes_are_vitae_immune() {
    let mut b = book(&[1, 2]);
    b.on_enchantment_added(vitae(1, 0.67));
    let mut s = LocalPlayerState::new(Some(b));
    s.on_attribute_update(1, 100, 100, 0);
    assert_eq!(
        s.get_effective_attribute(AttributeKind::Strength),
        Some(200)
    );
}

#[test]
fn attribute_buff_multiplies_and_truncates() {
    let mut b = book(&[1, 2]);
    b.on_enchantment_added(typed(2, 1, ATTRIBUTE, 1, 1.1, 1));
    let mut s = LocalPlayerState::new(Some(b));
    s.on_attribute_update(1, 100, 100, 0);
    assert_eq!(
        s.get_effective_attribute(AttributeKind::Strength),
        Some(220)
    );
}

#[test]
fn all_attribute_enchantment_stacks_and_feeds_health_and_skills() {
    let mut b = book(&[1, 2]);
    b.on_enchantment_added(typed(1, 1, ATTRIBUTE, 1, 40.0, 2));
    // A blessing that raises every attribute.
    b.on_enchantment_added(typed(
        2,
        2,
        (EnchantmentTypeFlags::ATTRIBUTE | EnchantmentTypeFlags::MULTIPLE_STAT).0,
        0,
        9.0,
        2,
    ));
    let mut s = LocalPlayerState::new(Some(b));
    s.set_skill_formula_bonus_resolver(Some(Arc::new(|_, _, attrs: &BTreeMap<u32, u32>| {
        attrs[&2]
    })));
    s.on_attribute_update(1, 0, 300, 0);
    s.on_attribute_update(2, 0, 200, 0);
    s.on_vital_update(7, 0, 0, 0, 0);
    s.on_skill_update(24, 0, 2, 0, 0, 0, 0.0, 200);

    assert_eq!(
        s.get_effective_attribute(AttributeKind::Strength),
        Some(349)
    );
    assert_eq!(s.get_base_max_approx(VitalKind::Health), Some(100));
    assert_eq!(s.get_max_approx(VitalKind::Health), Some(105));
    assert_eq!(s.attribute_enchantment_skill_delta(24), 9);
}

#[test]
fn effective_skill_without_spellbook_is_base() {
    let mut s = LocalPlayerState::default();
    s.on_skill_update(6, 300, 2, 0, 3, 0, 0.0, 0);
    assert_eq!(s.get_effective_skill(6), Some(303));
    assert_eq!(s.get_skill_vitae_modifier(6), 0);
}

#[test]
fn effective_skill_unseen_is_none() {
    let s = LocalPlayerState::new(Some(Spellbook::default()));
    assert_eq!(s.get_effective_skill(6), None);
}

#[test]
fn thirty_three_percent_vitae_golden() {
    let mut b = book(&[1, 2]);
    b.on_enchantment_added(vitae(1, 0.67));
    let mut s = LocalPlayerState::new(Some(b));
    s.on_skill_update(6, 300, 2, 0, 3, 0, 0.0, 0);
    assert_eq!(s.get_effective_skill(6), Some(203));
    assert_eq!(s.get_skill_vitae_modifier(6), -100);
}

#[test]
fn buff_plus_vitae_composition() {
    let mut b = book(&[1, 2]);
    b.on_enchantment_added(vitae(1, 0.67));
    b.on_enchantment_added(typed(2, 2, SKILL, 6, 50.0, 2));
    let mut s = LocalPlayerState::new(Some(b));
    s.on_skill_update(6, 297, 2, 0, 3, 0, 0.0, 0);
    assert_eq!(s.get_effective_skill(6), Some(251));
    assert_eq!(s.get_skill_vitae_modifier(6), -99);
}

#[test]
fn attribute_buffs_raise_the_formula_term() {
    let mut b = book(&[1, 2, 3]);
    b.on_enchantment_added(typed(1, 1, ATTRIBUTE, 5, 45.0, 2)); // Focus
    b.on_enchantment_added(typed(2, 2, ATTRIBUTE, 6, 45.0, 2)); // Self
    b.on_enchantment_added(typed(3, 3, SKILL, 0x1F, 50.0, 2));
    let mut s = LocalPlayerState::new(Some(b));
    s.set_skill_formula_bonus_resolver(Some(Arc::new(|skill, _, attrs: &BTreeMap<u32, u32>| {
        if skill == 0x1F {
            (f64::from(attrs[&5] + attrs[&6]) / 4.0 + 0.5).floor() as u32
        } else {
            0
        }
    })));
    s.on_attribute_update(5, 0, 251, 0);
    s.on_attribute_update(6, 0, 251, 0);
    s.on_skill_update(0x1F, 200, 2, 0, 15, 0, 0.0, 126);

    let value = s.get_skill_value(0x1F, None).expect("received");
    assert_eq!(s.attribute_enchantment_skill_delta(0x1F), 22);
    assert_eq!(value.unenchanted_level, 341);
    assert_eq!(value.effective_level, 413);
    assert_eq!(s.get_effective_skill(0x1F), Some(413));
}

#[test]
fn attribute_delta_needs_resolver_and_buffs() {
    let mut s = LocalPlayerState::new(Some(book(&[1])));
    s.on_attribute_update(3, 0, 100, 0);
    s.on_skill_update(24, 50, 2, 0, 0, 0, 0.0, 100);
    assert_eq!(s.attribute_enchantment_skill_delta(24), 0);

    s.set_skill_formula_bonus_resolver(Some(Arc::new(|_, _, attrs: &BTreeMap<u32, u32>| {
        attrs[&3]
    })));
    assert_eq!(s.attribute_enchantment_skill_delta(24), 0);
    assert_eq!(s.get_effective_skill(24), Some(150));

    s.spellbook_mut()
        .expect("spellbook")
        .on_enchantment_added(typed(1, 1, ATTRIBUTE, 3, 20.0, 2));
    assert_eq!(s.attribute_enchantment_skill_delta(24), 20);
    assert_eq!(s.get_effective_skill(24), Some(170));
    assert_eq!(
        s.get_skill_value(24, None)
            .expect("received")
            .unenchanted_level,
        150
    );
}

#[test]
fn wire_update_keeps_the_login_formula_bonus() {
    let mut s = LocalPlayerState::default();
    s.on_skill_update(6, 10, 2, 100, 0, 0, 0.0, 120);
    s.on_skill_wire_update(6, 11, 2, 2000, 0, 0, 5.0);
    let snap = s.skills()[&6];
    assert_eq!(snap.ranks, 11);
    assert_eq!(snap.xp, 2000);
    assert_eq!(snap.formula_bonus, 120);
    assert_eq!(snap.base_level(), 131);
}

#[test]
fn freshly_trained_skill_derives_its_term_through_the_resolver() {
    let mut s = LocalPlayerState::default();
    s.set_skill_formula_bonus_resolver(Some(Arc::new(
        |skill, _, attrs: &BTreeMap<u32, u32>| match (skill, attrs.get(&4)) {
            (33, Some(coordination)) => coordination / 4,
            _ => 0,
        },
    )));
    s.on_attribute_update(4, 0, 80, 0);
    s.on_skill_wire_update(33, 0, 2, 0, 0, 0, 0.0);
    assert_eq!(s.skills()[&33].formula_bonus, 20);
}

#[test]
fn movement_skill_totals() {
    let mut s = LocalPlayerState::default();
    assert_eq!(s.movement_skill_totals(), (-1, -1));
    s.on_skill_update(SKILL_RUN, 10, 2, 0, 5, 0, 0.0, 100);
    s.on_skill_update(SKILL_JUMP, 1, 2, 0, 2, 0, 0.0, 50);
    assert_eq!(s.movement_skill_totals(), (115, 53));
}

// SkillLevelConformanceTests: a maxed character, both attributes behind
// the skill at 315 so its formula term is 315.

const SKILL_ID: u32 = 24;
const TRAINED: u32 = 2;
const SPECIALIZED: u32 = 3;

fn maxed(spellbook: Option<Spellbook>) -> LocalPlayerState {
    let mut s = LocalPlayerState::new(spellbook);
    s.set_skill_formula_bonus_resolver(Some(Arc::new(
        |skill, _, _: &BTreeMap<u32, u32>| {
            if skill == SKILL_ID { 315 } else { 0 }
        },
    )));
    s.on_attribute_update(1, 215, 100, 0);
    s.on_attribute_update(4, 215, 100, 0);
    s
}

fn train(s: &mut LocalPlayerState, ranks: u32, init: u32, status: u32) {
    s.on_skill_wire_update(SKILL_ID, ranks, status, 0, init, 0, 0.0);
}

fn all_skills_five() -> AugmentationBonuses {
    AugmentationBonuses {
        all_skills: 5,
        ..AugmentationBonuses::default()
    }
}

#[test]
fn trained_at_the_ceiling() {
    let mut s = maxed(None);
    train(&mut s, 208, 0, TRAINED);
    let value = s.get_skill_value(SKILL_ID, None).expect("received");
    assert_eq!(value.unenchanted_level, 523);
    assert_eq!(value.effective_level, 523);
}

#[test]
fn specialized_carries_its_starting_value() {
    let mut s = maxed(None);
    train(&mut s, 226, 10, SPECIALIZED);
    let value = s.get_skill_value(SKILL_ID, None).expect("received");
    assert_eq!(value.unenchanted_level, 551);
    assert_eq!(value.effective_level, 551);
}

#[test]
fn all_skills_augmentation_adds_five_first() {
    for (ranks, init, status, expected) in [(208, 0, TRAINED, 528), (226, 10, SPECIALIZED, 556)] {
        let mut s = maxed(None);
        train(&mut s, ranks, init, status);
        let value = s
            .get_skill_value(SKILL_ID, Some(all_skills_five()))
            .expect("received");
        assert_eq!(value.unenchanted_level, expected);
        assert_eq!(value.effective_level, expected);
    }
}

#[test]
fn spells_scale_first_and_add_second() {
    let mut b = book(&[1, 2]);
    b.on_enchantment_added(typed(1, 1, SKILL, SKILL_ID, 1.15, 1));
    b.on_enchantment_added(typed(2, 2, SKILL, SKILL_ID, 20.0, 2));
    let mut s = maxed(Some(b));
    train(&mut s, 208, 0, TRAINED);
    let value = s.get_skill_value(SKILL_ID, None).expect("received");
    // 523 x 1.15 = 601.45, + 20 = 621.45.
    assert_eq!(value.unenchanted_level, 523);
    assert_eq!(value.effective_level, 621);
}

#[test]
fn vitae_scales_the_augmented_skill() {
    let mut b = book(&[1]);
    b.on_enchantment_added(ActiveEnchantmentRecord {
        stat_mod_value: Some(0.95),
        bucket: 4,
        ..ActiveEnchantmentRecord::new(1, 1, 0.0, 0)
    });
    let mut s = maxed(Some(b));
    train(&mut s, 208, 0, TRAINED);
    let value = s
        .get_skill_value(SKILL_ID, Some(all_skills_five()))
        .expect("received");
    // 523 + 5 = 528, and 528 x 0.95 = 501.6.
    assert_eq!(value.unenchanted_level, 528);
    assert_eq!(value.effective_level, 501);
    assert_eq!(value.vitae_modifier, -27);
}

#[test]
fn augmentations_read_from_int_properties() {
    let mut s = maxed(None);
    s.on_int_property_update(PropertyInt::LUM_AUG_ALL_SKILLS.0, 5);
    train(&mut s, 208, 0, TRAINED);
    assert_eq!(s.get_effective_skill(SKILL_ID), Some(528));
}

#[test]
fn the_servers_totals_replace_the_computed_attributes_and_vital_maxima() {
    let mut s = LocalPlayerState::new(Some(book(&[10])));
    s.on_attribute_update(AttributeKind::Coordination.id(), 100, 191, 0);
    s.on_vital_update(9, 0, 100, 0, 400);
    assert_eq!(
        s.get_effective_attribute(AttributeKind::Coordination),
        Some(291)
    );
    s.on_server_totals(ServerTotals {
        attributes: [0, 0, 0, 402, 0, 0],
        mana_max: 477,
        ..ServerTotals::default()
    });
    assert_eq!(
        s.get_effective_attribute(AttributeKind::Coordination),
        Some(402)
    );
    assert_eq!(s.get_max_approx(VitalKind::Mana), Some(477));
    // The unspelled base stays the character's own.
    assert_eq!(
        s.get_attribute(AttributeKind::Coordination)
            .unwrap()
            .current(),
        291
    );
    s.clear();
    assert_eq!(s.server_totals(), None);
}

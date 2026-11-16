//! 目標カロリーと PFC の表示の丸めの性質テスト。
//!
//! 対応する要求: `REQ-PRG-015`。

use proptest::prelude::*;
use warrior_core::nutrition::{display_macros, DietLossRange, MacroTarget, NutritionResult};

fn result(target_kcal: f64, protein_g: f64, fat_g: f64, carbohydrate_g: f64) -> NutritionResult {
    NutritionResult {
        age: 36,
        basal_metabolism_kcal: 1_600.0,
        maintenance_kcal: target_kcal,
        target_kcal,
        protein_g,
        fat_g,
        carbohydrate_g,
        macro_target: MacroTarget::default(),
        macro_clamped: false,
        diet_loss_per_week: None::<DietLossRange>,
    }
}

proptest! {
    #[test]
    fn req_prg_015_the_displayed_grams_stay_within_the_rounding_error(
        target_kcal in 1_000u32..5_000,
        protein_g in 0u32..200,
        fat_g in 0u32..150,
    ) {
        let protein_kcal = i64::from(protein_g) * 4;
        let fat_kcal = i64::from(fat_g) * 9;
        prop_assume!(protein_kcal + fat_kcal <= i64::from(target_kcal));
        let carbohydrate_g = (i64::from(target_kcal) - protein_kcal - fat_kcal) as f64 / 4.0;
        let displayed = display_macros(&result(
            f64::from(target_kcal),
            f64::from(protein_g),
            f64::from(fat_g),
            carbohydrate_g,
        ));
        prop_assert_eq!(displayed.target_kcal, target_kcal);
        prop_assert_eq!(displayed.protein_g, protein_g);
        prop_assert_eq!(displayed.fat_g, fat_g);
        prop_assert!(displayed.calorie_difference().abs() <= 2);
    }

    #[test]
    fn req_prg_016_the_macro_target_round_trips_through_clamping(
        protein in 1.0f64..3.0,
        fat in -0.2f64..1.2,
    ) {
        let (clamped, changed) = MacroTarget {
            protein_g_per_kg: protein,
            fat_ratio: fat,
        }
        .clamp();
        prop_assert!(clamped.is_valid());
        prop_assert_eq!(changed, clamped.protein_g_per_kg != protein || clamped.fat_ratio != fat);
    }
}

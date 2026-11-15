//! 人体図のインライン SVG と、筋肉の単位に応じた色分け。
//!
//! 前面と背面の SVG は `docs/design/assets/body-front.svg` と `body-back.svg` を
//! もとにしている。`docs/design/README.md` の「人体図の使い方」に従い、主働筋と
//! 補助筋の単位を `data-primary` と `data-secondary` に空白区切りで設定する。
//! 色は `docs/design/tokens.json` から生成した CSS カスタムプロパティで指定する
//! (ADR-012)。SVG の中の ID は `wfig-` で始め、前面と背面を同じページに置ける。

use warrior_core::exercise::{
    Attachment, BodyPart, Exercise, GripSelection, MuscleAssignment, MuscleRole, MuscleUnit,
};

/// 種目マスターの「シーテッドロウ」の ID。
///
/// PRD の「グリップによる色の変更」はシーテッドロウの手幅だけを対象にする。
/// 種目の名前は編集できるため、名前ではなく種目 ID で判定する。
pub const SEATED_ROW_EXERCISE_ID: &str = "a7c4e2f0-0000-4000-8000-00000000001b";

/// 人体図の面。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FigureSide {
    /// 前面。
    Front,
    /// 背面。
    Back,
}

impl FigureSide {
    /// `data-figure-side` の値。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Front => "front",
            Self::Back => "back",
        }
    }

    fn aria_label(self) -> &'static str {
        match self {
            Self::Front => "人体図（前面）",
            Self::Back => "人体図（背面）",
        }
    }

    fn body(self) -> &'static str {
        match self {
            Self::Front => FRONT_BODY,
            Self::Back => BACK_BODY,
        }
    }
}

/// 人体図で色を付ける筋肉。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FigureMuscles {
    /// 主働筋。補助筋と同じ単位を持つ場合は主働筋の色を優先する。
    pub primary: Vec<MuscleUnit>,
    /// 補助筋。
    pub secondary: Vec<MuscleUnit>,
}

/// 種目とグリップの選択から、人体図で色を付ける筋肉を決める。
///
/// グリップを指定しないときと、PRD の表にない種目や組み合わせのときは、
/// 種目が持つ主働筋と補助筋をそのまま返す。筋肉の単位を持たない独自種目は、
/// 部位区分に属する単位を主働筋にする (ADR-012)。
pub fn figure_muscles(exercise: &Exercise, grip: Option<GripSelection>) -> FigureMuscles {
    if exercise.body_part.is_other() {
        return FigureMuscles::default();
    }
    if let Some(muscles) = seated_row_grip_muscles(exercise, grip) {
        return muscles;
    }
    if exercise.muscles.is_empty() {
        return FigureMuscles {
            primary: body_part_units(exercise.body_part),
            secondary: Vec::new(),
        };
    }
    FigureMuscles {
        primary: units_with_role(&exercise.muscles, MuscleRole::Primary),
        secondary: units_with_role(&exercise.muscles, MuscleRole::Secondary),
    }
}

/// シーテッドロウの手幅による主働筋と補助筋の変更。
///
/// PRD の「グリップによる色の変更」の 3 行だけを適用する。狭いアタッチメントは
/// `Attachment::is_narrow` が表し、広いアタッチメントはワイドバーとマグワイド
/// オーバーの 2 種である。
fn seated_row_grip_muscles(
    exercise: &Exercise,
    grip: Option<GripSelection>,
) -> Option<FigureMuscles> {
    if exercise.id != SEATED_ROW_EXERCISE_ID {
        return None;
    }
    let Some(GripSelection::Attachment { attachment, .. }) = grip else {
        return None;
    };
    let (primary, secondary) = if attachment.is_narrow() {
        (
            vec![MuscleUnit::LatissimusDorsi],
            vec![MuscleUnit::TrapeziusMiddleLower],
        )
    } else if matches!(attachment, Attachment::WideBar | Attachment::MagWideOver) {
        (
            vec![MuscleUnit::TrapeziusUpper, MuscleUnit::TrapeziusMiddleLower],
            vec![MuscleUnit::LatissimusDorsi],
        )
    } else {
        (
            vec![
                MuscleUnit::LatissimusDorsi,
                MuscleUnit::TrapeziusMiddleLower,
            ],
            Vec::new(),
        )
    };
    Some(FigureMuscles { primary, secondary })
}

fn units_with_role(muscles: &[MuscleAssignment], role: MuscleRole) -> Vec<MuscleUnit> {
    muscles
        .iter()
        .filter(|assignment| assignment.role == role)
        .map(|assignment| assignment.muscle)
        .collect()
}

fn body_part_units(body_part: BodyPart) -> Vec<MuscleUnit> {
    MuscleUnit::ALL
        .into_iter()
        .filter(|unit| unit.body_part() == body_part)
        .collect()
}

/// 筋肉の単位に対応する SVG の `data-unit` の値。
pub fn unit_attribute(unit: MuscleUnit) -> &'static str {
    match unit {
        MuscleUnit::PectoralisMajorUpper => "pec-upper",
        MuscleUnit::PectoralisMajorMiddle => "pec-mid",
        MuscleUnit::PectoralisMajorLower => "pec-lower",
        MuscleUnit::LatissimusDorsi => "lats",
        MuscleUnit::TeresMajor => "teres-major",
        MuscleUnit::TrapeziusUpper => "trap-upper",
        MuscleUnit::TrapeziusMiddleLower => "trap-mid-lower",
        MuscleUnit::ErectorSpinae => "erector-spinae",
        MuscleUnit::AnteriorDeltoid => "delt-front",
        MuscleUnit::LateralDeltoid => "delt-mid",
        MuscleUnit::PosteriorDeltoid => "delt-rear",
        MuscleUnit::BicepsBrachii => "biceps",
        MuscleUnit::Forearm => "forearm",
        MuscleUnit::TricepsBrachii => "triceps",
        MuscleUnit::Quadriceps => "quads",
        MuscleUnit::Hamstrings => "hamstrings",
        MuscleUnit::GluteusMaximus => "glute-max",
        MuscleUnit::GluteusMedius => "glute-med",
        MuscleUnit::Adductors => "adductors",
        MuscleUnit::Gastrocnemius => "gastrocnemius",
        MuscleUnit::Soleus => "soleus",
        MuscleUnit::RectusAbdominisUpper => "abs-upper",
        MuscleUnit::RectusAbdominisLower => "abs-lower",
        MuscleUnit::Obliques => "obliques",
    }
}

/// 主働筋と補助筋を設定した 1 面の SVG。
///
/// `data-primary` と `data-secondary` は SVG の要素ではなくルートに置く。
/// 要素は自分の単位がどちらかに含まれる場合に色を変える。
pub fn figure_svg(side: FigureSide, muscles: &FigureMuscles) -> String {
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" class="wfig block h-auto w-full" viewBox="0 0 200 440" role="img" aria-label="{}" data-figure-side="{}" data-primary="{}" data-secondary="{}">{}{}</svg>"##,
        side.aria_label(),
        side.as_str(),
        attribute_value(&muscles.primary),
        attribute_value(&muscles.secondary),
        FIGURE_STYLE,
        side.body(),
    )
}

fn attribute_value(units: &[MuscleUnit]) -> String {
    units
        .iter()
        .map(|unit| unit_attribute(*unit))
        .collect::<Vec<_>>()
        .join(" ")
}

/// `docs/design/assets/body-front.svg` と `body-back.svg` が共有するスタイル。
///
/// 色の値はトークンの CSS カスタムプロパティが持つため、SVG の中に色を書かない。
/// 変数の指定が無い要素はテーマの値に従う (ADR-011、ADR-012)。
const FIGURE_STYLE: &str = r##"<style>
.wfig-body{fill:var(--fig-body)}
.wfig-tendon{fill:var(--fig-tendon)}
.wfig-m{fill:var(--fig-rest)}
.wfig-f{fill:none;stroke:var(--fig-rest-line);stroke-width:.9}
.wfig-rim{fill:none;stroke:var(--fig-rim);stroke-width:8;opacity:var(--fig-rim-opacity,0.12)}
.wfig-m[data-unit="pec-upper"]{fill:var(--wfig-pec-upper,var(--fig-rest))}
.wfig-f[data-unit="pec-upper"]{stroke:var(--wfig-pec-upper-line,var(--fig-rest-line))}
.wfig-m[data-unit="pec-mid"]{fill:var(--wfig-pec-mid,var(--fig-rest))}
.wfig-f[data-unit="pec-mid"]{stroke:var(--wfig-pec-mid-line,var(--fig-rest-line))}
.wfig-m[data-unit="pec-lower"]{fill:var(--wfig-pec-lower,var(--fig-rest))}
.wfig-f[data-unit="pec-lower"]{stroke:var(--wfig-pec-lower-line,var(--fig-rest-line))}
.wfig-m[data-unit="lats"]{fill:var(--wfig-lats,var(--fig-rest))}
.wfig-f[data-unit="lats"]{stroke:var(--wfig-lats-line,var(--fig-rest-line))}
.wfig-m[data-unit="teres-major"]{fill:var(--wfig-teres-major,var(--fig-rest))}
.wfig-f[data-unit="teres-major"]{stroke:var(--wfig-teres-major-line,var(--fig-rest-line))}
.wfig-m[data-unit="trap-upper"]{fill:var(--wfig-trap-upper,var(--fig-rest))}
.wfig-f[data-unit="trap-upper"]{stroke:var(--wfig-trap-upper-line,var(--fig-rest-line))}
.wfig-m[data-unit="trap-mid-lower"]{fill:var(--wfig-trap-mid-lower,var(--fig-rest))}
.wfig-f[data-unit="trap-mid-lower"]{stroke:var(--wfig-trap-mid-lower-line,var(--fig-rest-line))}
.wfig-m[data-unit="erector-spinae"]{fill:var(--wfig-erector-spinae,var(--fig-rest))}
.wfig-f[data-unit="erector-spinae"]{stroke:var(--wfig-erector-spinae-line,var(--fig-rest-line))}
.wfig-m[data-unit="delt-front"]{fill:var(--wfig-delt-front,var(--fig-rest))}
.wfig-f[data-unit="delt-front"]{stroke:var(--wfig-delt-front-line,var(--fig-rest-line))}
.wfig-m[data-unit="delt-mid"]{fill:var(--wfig-delt-mid,var(--fig-rest))}
.wfig-f[data-unit="delt-mid"]{stroke:var(--wfig-delt-mid-line,var(--fig-rest-line))}
.wfig-m[data-unit="delt-rear"]{fill:var(--wfig-delt-rear,var(--fig-rest))}
.wfig-f[data-unit="delt-rear"]{stroke:var(--wfig-delt-rear-line,var(--fig-rest-line))}
.wfig-m[data-unit="biceps"]{fill:var(--wfig-biceps,var(--fig-rest))}
.wfig-f[data-unit="biceps"]{stroke:var(--wfig-biceps-line,var(--fig-rest-line))}
.wfig-m[data-unit="forearm"]{fill:var(--wfig-forearm,var(--fig-rest))}
.wfig-f[data-unit="forearm"]{stroke:var(--wfig-forearm-line,var(--fig-rest-line))}
.wfig-m[data-unit="triceps"]{fill:var(--wfig-triceps,var(--fig-rest))}
.wfig-f[data-unit="triceps"]{stroke:var(--wfig-triceps-line,var(--fig-rest-line))}
.wfig-m[data-unit="quads"]{fill:var(--wfig-quads,var(--fig-rest))}
.wfig-f[data-unit="quads"]{stroke:var(--wfig-quads-line,var(--fig-rest-line))}
.wfig-m[data-unit="hamstrings"]{fill:var(--wfig-hamstrings,var(--fig-rest))}
.wfig-f[data-unit="hamstrings"]{stroke:var(--wfig-hamstrings-line,var(--fig-rest-line))}
.wfig-m[data-unit="glute-max"]{fill:var(--wfig-glute-max,var(--fig-rest))}
.wfig-f[data-unit="glute-max"]{stroke:var(--wfig-glute-max-line,var(--fig-rest-line))}
.wfig-m[data-unit="glute-med"]{fill:var(--wfig-glute-med,var(--fig-rest))}
.wfig-f[data-unit="glute-med"]{stroke:var(--wfig-glute-med-line,var(--fig-rest-line))}
.wfig-m[data-unit="adductors"]{fill:var(--wfig-adductors,var(--fig-rest))}
.wfig-f[data-unit="adductors"]{stroke:var(--wfig-adductors-line,var(--fig-rest-line))}
.wfig-m[data-unit="gastrocnemius"]{fill:var(--wfig-gastrocnemius,var(--fig-rest))}
.wfig-f[data-unit="gastrocnemius"]{stroke:var(--wfig-gastrocnemius-line,var(--fig-rest-line))}
.wfig-m[data-unit="soleus"]{fill:var(--wfig-soleus,var(--fig-rest))}
.wfig-f[data-unit="soleus"]{stroke:var(--wfig-soleus-line,var(--fig-rest-line))}
.wfig-m[data-unit="abs-upper"]{fill:var(--wfig-abs-upper,var(--fig-rest))}
.wfig-f[data-unit="abs-upper"]{stroke:var(--wfig-abs-upper-line,var(--fig-rest-line))}
.wfig-m[data-unit="abs-lower"]{fill:var(--wfig-abs-lower,var(--fig-rest))}
.wfig-f[data-unit="abs-lower"]{stroke:var(--wfig-abs-lower-line,var(--fig-rest-line))}
.wfig-m[data-unit="obliques"]{fill:var(--wfig-obliques,var(--fig-rest))}
.wfig-f[data-unit="obliques"]{stroke:var(--wfig-obliques-line,var(--fig-rest-line))}
.wfig[data-secondary~="pec-upper"]{--wfig-pec-upper:var(--fig-secondary);--wfig-pec-upper-line:var(--fig-secondary-line)}
.wfig[data-secondary~="pec-mid"]{--wfig-pec-mid:var(--fig-secondary);--wfig-pec-mid-line:var(--fig-secondary-line)}
.wfig[data-secondary~="pec-lower"]{--wfig-pec-lower:var(--fig-secondary);--wfig-pec-lower-line:var(--fig-secondary-line)}
.wfig[data-secondary~="lats"]{--wfig-lats:var(--fig-secondary);--wfig-lats-line:var(--fig-secondary-line)}
.wfig[data-secondary~="teres-major"]{--wfig-teres-major:var(--fig-secondary);--wfig-teres-major-line:var(--fig-secondary-line)}
.wfig[data-secondary~="trap-upper"]{--wfig-trap-upper:var(--fig-secondary);--wfig-trap-upper-line:var(--fig-secondary-line)}
.wfig[data-secondary~="trap-mid-lower"]{--wfig-trap-mid-lower:var(--fig-secondary);--wfig-trap-mid-lower-line:var(--fig-secondary-line)}
.wfig[data-secondary~="erector-spinae"]{--wfig-erector-spinae:var(--fig-secondary);--wfig-erector-spinae-line:var(--fig-secondary-line)}
.wfig[data-secondary~="delt-front"]{--wfig-delt-front:var(--fig-secondary);--wfig-delt-front-line:var(--fig-secondary-line)}
.wfig[data-secondary~="delt-mid"]{--wfig-delt-mid:var(--fig-secondary);--wfig-delt-mid-line:var(--fig-secondary-line)}
.wfig[data-secondary~="delt-rear"]{--wfig-delt-rear:var(--fig-secondary);--wfig-delt-rear-line:var(--fig-secondary-line)}
.wfig[data-secondary~="biceps"]{--wfig-biceps:var(--fig-secondary);--wfig-biceps-line:var(--fig-secondary-line)}
.wfig[data-secondary~="forearm"]{--wfig-forearm:var(--fig-secondary);--wfig-forearm-line:var(--fig-secondary-line)}
.wfig[data-secondary~="triceps"]{--wfig-triceps:var(--fig-secondary);--wfig-triceps-line:var(--fig-secondary-line)}
.wfig[data-secondary~="quads"]{--wfig-quads:var(--fig-secondary);--wfig-quads-line:var(--fig-secondary-line)}
.wfig[data-secondary~="hamstrings"]{--wfig-hamstrings:var(--fig-secondary);--wfig-hamstrings-line:var(--fig-secondary-line)}
.wfig[data-secondary~="glute-max"]{--wfig-glute-max:var(--fig-secondary);--wfig-glute-max-line:var(--fig-secondary-line)}
.wfig[data-secondary~="glute-med"]{--wfig-glute-med:var(--fig-secondary);--wfig-glute-med-line:var(--fig-secondary-line)}
.wfig[data-secondary~="adductors"]{--wfig-adductors:var(--fig-secondary);--wfig-adductors-line:var(--fig-secondary-line)}
.wfig[data-secondary~="gastrocnemius"]{--wfig-gastrocnemius:var(--fig-secondary);--wfig-gastrocnemius-line:var(--fig-secondary-line)}
.wfig[data-secondary~="soleus"]{--wfig-soleus:var(--fig-secondary);--wfig-soleus-line:var(--fig-secondary-line)}
.wfig[data-secondary~="abs-upper"]{--wfig-abs-upper:var(--fig-secondary);--wfig-abs-upper-line:var(--fig-secondary-line)}
.wfig[data-secondary~="abs-lower"]{--wfig-abs-lower:var(--fig-secondary);--wfig-abs-lower-line:var(--fig-secondary-line)}
.wfig[data-secondary~="obliques"]{--wfig-obliques:var(--fig-secondary);--wfig-obliques-line:var(--fig-secondary-line)}
.wfig[data-primary~="pec-upper"]{--wfig-pec-upper:var(--fig-primary);--wfig-pec-upper-line:var(--fig-primary-line)}
.wfig[data-primary~="pec-mid"]{--wfig-pec-mid:var(--fig-primary);--wfig-pec-mid-line:var(--fig-primary-line)}
.wfig[data-primary~="pec-lower"]{--wfig-pec-lower:var(--fig-primary);--wfig-pec-lower-line:var(--fig-primary-line)}
.wfig[data-primary~="lats"]{--wfig-lats:var(--fig-primary);--wfig-lats-line:var(--fig-primary-line)}
.wfig[data-primary~="teres-major"]{--wfig-teres-major:var(--fig-primary);--wfig-teres-major-line:var(--fig-primary-line)}
.wfig[data-primary~="trap-upper"]{--wfig-trap-upper:var(--fig-primary);--wfig-trap-upper-line:var(--fig-primary-line)}
.wfig[data-primary~="trap-mid-lower"]{--wfig-trap-mid-lower:var(--fig-primary);--wfig-trap-mid-lower-line:var(--fig-primary-line)}
.wfig[data-primary~="erector-spinae"]{--wfig-erector-spinae:var(--fig-primary);--wfig-erector-spinae-line:var(--fig-primary-line)}
.wfig[data-primary~="delt-front"]{--wfig-delt-front:var(--fig-primary);--wfig-delt-front-line:var(--fig-primary-line)}
.wfig[data-primary~="delt-mid"]{--wfig-delt-mid:var(--fig-primary);--wfig-delt-mid-line:var(--fig-primary-line)}
.wfig[data-primary~="delt-rear"]{--wfig-delt-rear:var(--fig-primary);--wfig-delt-rear-line:var(--fig-primary-line)}
.wfig[data-primary~="biceps"]{--wfig-biceps:var(--fig-primary);--wfig-biceps-line:var(--fig-primary-line)}
.wfig[data-primary~="forearm"]{--wfig-forearm:var(--fig-primary);--wfig-forearm-line:var(--fig-primary-line)}
.wfig[data-primary~="triceps"]{--wfig-triceps:var(--fig-primary);--wfig-triceps-line:var(--fig-primary-line)}
.wfig[data-primary~="quads"]{--wfig-quads:var(--fig-primary);--wfig-quads-line:var(--fig-primary-line)}
.wfig[data-primary~="hamstrings"]{--wfig-hamstrings:var(--fig-primary);--wfig-hamstrings-line:var(--fig-primary-line)}
.wfig[data-primary~="glute-max"]{--wfig-glute-max:var(--fig-primary);--wfig-glute-max-line:var(--fig-primary-line)}
.wfig[data-primary~="glute-med"]{--wfig-glute-med:var(--fig-primary);--wfig-glute-med-line:var(--fig-primary-line)}
.wfig[data-primary~="adductors"]{--wfig-adductors:var(--fig-primary);--wfig-adductors-line:var(--fig-primary-line)}
.wfig[data-primary~="gastrocnemius"]{--wfig-gastrocnemius:var(--fig-primary);--wfig-gastrocnemius-line:var(--fig-primary-line)}
.wfig[data-primary~="soleus"]{--wfig-soleus:var(--fig-primary);--wfig-soleus-line:var(--fig-primary-line)}
.wfig[data-primary~="abs-upper"]{--wfig-abs-upper:var(--fig-primary);--wfig-abs-upper-line:var(--fig-primary-line)}
.wfig[data-primary~="abs-lower"]{--wfig-abs-lower:var(--fig-primary);--wfig-abs-lower-line:var(--fig-primary-line)}
.wfig[data-primary~="obliques"]{--wfig-obliques:var(--fig-primary);--wfig-obliques-line:var(--fig-primary-line)}
</style>"##;

const FRONT_BODY: &str = r##"<defs>
<path id="wfig-bf-sil" d="M100.5 4C90 4 82 12.5 82 27C82 39 86 50.5 93 57.5L92 65C91 68.5 88 71 84 73L63 80C54 80 46 88 45 100C44 110 43 120 43 130C42 145 41 158 38 170C34 180 32 190 32 202C32 214 32 224 32 234C28 244 29 260 34 268C39 272 43 264 43 254L42 234C45 222 47 210 48 198C49 188 50 180 51 172C54 160 57 146 58 132C59 126 60 120 62 116C63 124 64 130 64 136C65 148 68 158 69 168C67 178 65 186 65 193C63 200 61 208 60 216C58 232 58 250 61 270C63 286 66 298 68 310C69 316 68 322 66 328C62 338 62 352 65 364C67 378 70 392 72 404C70 412 66 420 66 426C66 432 72 434 78 434L86 434C88 428 86 416 84 404C85 390 87 376 88 362C90 350 90 338 88 326C87 320 87 314 88 308C91 290 94 270 96 250C97 242 99 236 100.5 230"></path>
<clipPath id="wfig-bf-sc"><use href="#wfig-bf-sil"></use></clipPath>
<g id="wfig-bf-f">
<use href="#wfig-bf-sil" class="wfig-body"></use>
<path id="wfig-f-sc" d="M93.4 59.5L96.4 60.5C96.8 68 98 76 99.5 84L96 84C94.6 76 93.6 68 93.4 59.5Z" class="wfig-m"></path>
<path id="wfig-f-tu" d="M91 66C90 70 87 72 84 73.5L64 80.5L66 82.5C74 82 84 81 90 80.5C91 76 91.5 71 91 66Z" class="wfig-m" data-unit="trap-upper"></path>
<clipPath id="wfig-fc-tu"><use href="#wfig-f-tu"></use></clipPath>
<g clip-path="url(#wfig-fc-tu)" class="wfig-f" data-unit="trap-upper"><path d="M90.5 69.5L65 81.5M90.8 76L74 82.3"></path></g>
<path id="wfig-f-pu" d="M74 84.5L98 86.5L98 97C88 98 75 103 63 111C67 102 71 93 74 84.5Z" class="wfig-m" data-unit="pec-upper"></path>
<clipPath id="wfig-fc-pu"><use href="#wfig-f-pu"></use></clipPath>
<g clip-path="url(#wfig-fc-pu)" class="wfig-f" data-unit="pec-upper"><path d="M79 85L62 111M87 85.7L62.4 111.2M95 86.3L62.8 111.4M98.5 92L63.4 111.4"></path></g>
<path id="wfig-f-pm" d="M98 98.5C88 99.5 75 104.5 63 112.5L63.5 115C75 119.5 88 119.5 98 117Z" class="wfig-m" data-unit="pec-mid"></path>
<clipPath id="wfig-fc-pm"><use href="#wfig-f-pm"></use></clipPath>
<g clip-path="url(#wfig-fc-pm)" class="wfig-f" data-unit="pec-mid"><path d="M98.5 101L62 113.2M98.5 107L62 113.8M98.5 113L62 114.4"></path></g>
<path id="wfig-f-pl" d="M98 118.5C88 121 75 121 64 116.5C66 125 76 132 88 132C93 132 97 130 98 128Z" class="wfig-m" data-unit="pec-lower"></path>
<clipPath id="wfig-fc-pl"><use href="#wfig-f-pl"></use></clipPath>
<g clip-path="url(#wfig-fc-pl)" class="wfig-f" data-unit="pec-lower"><path d="M98.5 121L62.5 116M96 127.5L62.8 116.6M86 132L63.2 117.2M74 127L63.6 117.6"></path></g>
<path id="wfig-f-da" d="M73 84L62 82.5C56 92 52 108 50.5 128C56 122 61 113 65 104C68 97 71 90 73 84Z" class="wfig-m" data-unit="delt-front"></path>
<clipPath id="wfig-fc-da"><use href="#wfig-f-da"></use></clipPath>
<g clip-path="url(#wfig-fc-da)" class="wfig-f" data-unit="delt-front"><path d="M71.5 85.5Q63 104 51.2 126.5M66.5 84.5Q58 102 50.8 125"></path></g>
<path id="wfig-f-dm" d="M61 82C53 82 46.5 90 45.5 100C44.5 110 46 121 50 128C51.5 108 55.5 92 61 82Z" class="wfig-m" data-unit="delt-mid"></path>
<clipPath id="wfig-fc-dm"><use href="#wfig-f-dm"></use></clipPath>
<g clip-path="url(#wfig-fc-dm)" class="wfig-f" data-unit="delt-mid"><path d="M58.5 83.5Q51.5 100 50 126M52.5 86Q47 102 48.8 123"></path></g>
<path d="M66 126L73 131L72 134L65.5 130ZM66 133L74 138L73 141L66 137ZM67 140L75 145L74 148L67.5 144Z" class="wfig-m"></path>
<path id="wfig-f-au" d="M89 133H98V145H86V136A3 3 0 0 1 89 133ZM86 147H98V160H86ZM86 162H98V176H89A3 3 0 0 1 86 173Z" class="wfig-m" data-unit="abs-upper"></path>
<clipPath id="wfig-fc-au"><use href="#wfig-f-au"></use></clipPath>
<g clip-path="url(#wfig-fc-au)" class="wfig-f" data-unit="abs-upper"><path d="M88.5 132V177M93.5 132V177"></path></g>
<path id="wfig-f-al" d="M86 179L98 179L98 224C93 222 89.5 213 88 203C86.8 195 86 187 86 179Z" class="wfig-m" data-unit="abs-lower"></path>
<clipPath id="wfig-fc-al"><use href="#wfig-f-al"></use></clipPath>
<g clip-path="url(#wfig-fc-al)" class="wfig-f" data-unit="abs-lower"><path d="M88.5 178V224M93.5 178V224"></path></g>
<path id="wfig-f-ob" d="M84 135C80 134 77 136 75 139C71 145 67 150 66.5 153C67.5 159 69 164 69.2 168C67.5 177 65.5 185 65.8 192.5C72 196 78 199 84 202Z" class="wfig-m" data-unit="obliques"></path>
<clipPath id="wfig-fc-ob"><use href="#wfig-f-ob"></use></clipPath>
<g clip-path="url(#wfig-fc-ob)" class="wfig-f" data-unit="obliques"><path d="M62 130L86 167M62 146L86 183M62 162L86 199M62 178L80 206"></path></g>
<path id="wfig-f-bi" d="M59.5 116C59.5 124 58 134 56.5 142C55 152 52.5 161 49.5 168L46.5 167C45.5 158 46 146 47.5 136C49 127 52 120 56 115.5Z" class="wfig-m" data-unit="biceps"></path>
<clipPath id="wfig-fc-bi"><use href="#wfig-f-bi"></use></clipPath>
<g clip-path="url(#wfig-fc-bi)" class="wfig-f" data-unit="biceps"><path d="M54 110L45 170M59 110L50 170"></path></g>
<path id="wfig-f-tr" d="M49 130C47 127 45 127 43.8 131C42.8 141 42.2 152 41.5 160L45 163C45 152 45.8 141 47.5 133Z" class="wfig-m" data-unit="triceps"></path>
<clipPath id="wfig-fc-tr"><use href="#wfig-f-tr"></use></clipPath>
<g clip-path="url(#wfig-fc-tr)" class="wfig-f" data-unit="triceps"><path d="M46.5 126L43.5 164"></path></g>
<path id="wfig-f-fa" d="M39.5 171C35 180 33 191 33 202C33 214 33 224 33 233L41.5 233C44.5 222 46.5 210 47.5 198C48.5 188 49.5 180 50.5 173C47 171.5 43 170.5 39.5 171Z" class="wfig-m" data-unit="forearm"></path>
<clipPath id="wfig-fc-fa"><use href="#wfig-f-fa"></use></clipPath>
<g clip-path="url(#wfig-fc-fa)" class="wfig-f" data-unit="forearm"><path d="M40 168L34.5 236M46 168L38 236M52 170L41.5 236"></path></g>
<path id="wfig-f-vl" d="M68 211C63 216 60.5 228 59.6 242C59.2 258 61 276 64.5 292C66 298 67.5 304 69.5 308L72 304.5C69 290 67.2 264 67.5 242C67.7 230 68.6 219 70 211.5Z" class="wfig-m" data-unit="quads"></path>
<clipPath id="wfig-fc-vl"><use href="#wfig-f-vl"></use></clipPath>
<g clip-path="url(#wfig-fc-vl)" class="wfig-f" data-unit="quads"><path d="M57 210L65 312M63 210L71 312M69 210L77 312"></path></g>
<path id="wfig-f-rf" d="M72 207C78 220 84.5 248 85 274C85.2 286 83.5 296 81.5 303L75 303.5C71.5 290 68.8 264 69 242C69.2 228 70.4 215 72 207Z" class="wfig-m" data-unit="quads"></path>
<clipPath id="wfig-fc-rf"><use href="#wfig-f-rf"></use></clipPath>
<g clip-path="url(#wfig-fc-rf)" class="wfig-f" data-unit="quads"><path d="M70 205L77 305M76 205L83 305"></path></g>
<path id="wfig-f-vm" d="M86.5 262C90 271 91.5 284 90.5 296C89.8 304 87 309.5 83 309L82.8 303.5C85.3 295 86.6 281 86.5 262Z" class="wfig-m" data-unit="quads"></path>
<clipPath id="wfig-fc-vm"><use href="#wfig-f-vm"></use></clipPath>
<g clip-path="url(#wfig-fc-vm)" class="wfig-f" data-unit="quads"><path d="M92 262L80 290M94 274L82 302"></path></g>
<path id="wfig-f-ad" d="M97.5 230C96.5 250 94.5 270 91 288C87.5 270 82.5 246 78 218C85 221 92 225.5 97.5 230Z" class="wfig-m" data-unit="adductors"></path>
<clipPath id="wfig-fc-ad"><use href="#wfig-f-ad"></use></clipPath>
<g clip-path="url(#wfig-fc-ad)" class="wfig-f" data-unit="adductors"><path d="M98 229L78 224M98 229L83 250M98 229L88 276"></path></g>
<path id="wfig-f-tf" d="M65.6 195C68.5 197.5 71.5 201 72.6 204.5C71.4 207 69.6 209.6 67.8 211.6C65.6 213.5 63 216 61.4 219C61.6 210 63.2 201.5 65.6 195Z" class="wfig-m"></path>
<clipPath id="wfig-fc-tf"><use href="#wfig-f-tf"></use></clipPath>
<g clip-path="url(#wfig-fc-tf)" class="wfig-f"><path d="M67 194L62 220M71 198L65 216"></path></g>
<path d="M77.5 306C81 305 84 307 84.5 311C85 316 82 320 78.5 320C75 320 72.5 316.5 73 312C73.3 309 75 307 77.5 306ZM76.6 321L80.4 321L80 328L77 328Z" class="wfig-tendon"></path>
<path id="wfig-f-pe" d="M66.6 330C64.2 342 64.6 356 66.6 368C68 380 70 390 71.8 400L73.2 396C71.2 384 69.4 372 68.8 360C68.4 350 68.4 340 69 331.5Z" class="wfig-m"></path>
<clipPath id="wfig-fc-pe"><use href="#wfig-f-pe"></use></clipPath>
<g clip-path="url(#wfig-fc-pe)" class="wfig-f"><path d="M66.6 330L70.5 400"></path></g>
<path id="wfig-f-ta" d="M71 327.5C75 338 77.2 356 77.2 374C77.2 384 76.2 392 75 398L73.6 393C72.4 382 70.8 368 70.3 352C70 342 70.2 334 71 327.5Z" class="wfig-m"></path>
<clipPath id="wfig-fc-ta"><use href="#wfig-f-ta"></use></clipPath>
<g clip-path="url(#wfig-fc-ta)" class="wfig-f"><path d="M71.5 326L74.6 399M75 332L76.6 399"></path></g>
<path id="wfig-f-ga" d="M87.6 327.5C89.6 338 89.6 351 87.6 363.5L83.8 361C84.6 350 84.6 338 83.8 329.5Z" class="wfig-m" data-unit="gastrocnemius"></path>
<clipPath id="wfig-fc-ga"><use href="#wfig-f-ga"></use></clipPath>
<g clip-path="url(#wfig-fc-ga)" class="wfig-f" data-unit="gastrocnemius"><path d="M86 327L86 365"></path></g>
<path id="wfig-f-so" d="M87.4 365.5C87 377 86 390 84.8 400L82.6 398C83.2 388 84 376 84.3 363.5Z" class="wfig-m" data-unit="soleus"></path>
<clipPath id="wfig-fc-so"><use href="#wfig-f-so"></use></clipPath>
<g clip-path="url(#wfig-fc-so)" class="wfig-f" data-unit="soleus"><path d="M85.8 364L83.6 400"></path></g>
<use href="#wfig-bf-sil" clip-path="url(#wfig-bf-sc)" class="wfig-rim"></use>
</g>
</defs>
<use href="#wfig-bf-f"></use>
<use href="#wfig-bf-f" transform="translate(200 0) scale(-1 1)"></use>"##;

const BACK_BODY: &str = r##"<defs>
<path id="wfig-bb-sil" d="M100.5 4C90 4 82 12.5 82 27C82 39 86 50.5 93 57.5L92 65C91 68.5 88 71 84 73L63 80C54 80 46 88 45 100C44 110 43 120 43 130C42 145 41 158 38 170C34 180 32 190 32 202C32 214 32 224 32 234C28 244 29 260 34 268C39 272 43 264 43 254L42 234C45 222 47 210 48 198C49 188 50 180 51 172C54 160 57 146 58 132C59 126 60 120 62 116C63 124 64 130 64 136C65 148 68 158 69 168C67 178 65 186 65 193C63 200 61 208 60 216C58 232 58 250 61 270C63 286 66 298 68 310C69 316 68 322 66 328C62 338 62 352 65 364C67 378 70 392 72 404C70 412 66 420 66 426C66 432 72 434 78 434L86 434C88 428 86 416 84 404C85 390 87 376 88 362C90 350 90 338 88 326C87 320 87 314 88 308C91 290 94 270 96 250C97 242 99 236 100.5 230"></path>
<clipPath id="wfig-bb-sc"><use href="#wfig-bb-sil"></use></clipPath>
<g id="wfig-bf-b">
<use href="#wfig-bb-sil" class="wfig-body"></use>
<path id="wfig-b-tu" d="M100.5 54L94 55.5C93 62 91 68 84 73L64 80.5L64.5 85C76 87.5 90 86.5 100.5 84.5Z" class="wfig-m" data-unit="trap-upper"></path>
<clipPath id="wfig-bc-tu"><use href="#wfig-b-tu"></use></clipPath>
<g clip-path="url(#wfig-bc-tu)" class="wfig-f" data-unit="trap-upper"><path d="M100.5 60L65 82M100.5 72L65 84M100.5 82L66 85.5"></path></g>
<path id="wfig-b-tl" d="M100.5 86.5C90 88.5 77 89.5 65.5 87L67 91C73 95 78.5 99 81 103C89 113 96.5 133 100.5 154Z" class="wfig-m" data-unit="trap-mid-lower"></path>
<clipPath id="wfig-bc-tl"><use href="#wfig-b-tl"></use></clipPath>
<g clip-path="url(#wfig-bc-tl)" class="wfig-f" data-unit="trap-mid-lower"><path d="M100.5 90L67 89M100.5 102L78 99.5M100.5 118L82 104M100.5 136L86 112M100.5 150L90 124"></path></g>
<path id="wfig-b-dp" d="M73.5 93.5L64 86.5L62 83.5C56.5 92 52 108 50.5 128C56 122 62 112 66.5 104C69.5 99 72 96 73.5 93.5Z" class="wfig-m" data-unit="delt-rear"></path>
<clipPath id="wfig-bc-dp"><use href="#wfig-b-dp"></use></clipPath>
<g clip-path="url(#wfig-bc-dp)" class="wfig-f" data-unit="delt-rear"><path d="M72 94Q62 106 51.4 126.5M66 88.5Q57 103 50.9 125"></path></g>
<path id="wfig-b-dm" d="M61 82C53 82 46.5 90 45.5 100C44.5 110 46 121 50 128C51.5 108 55.5 92 61 82Z" class="wfig-m" data-unit="delt-mid"></path>
<clipPath id="wfig-bc-dm"><use href="#wfig-b-dm"></use></clipPath>
<g clip-path="url(#wfig-bc-dm)" class="wfig-f" data-unit="delt-mid"><path d="M58.5 83.5Q51.5 100 50 126M52.5 86Q47 102 48.8 123"></path></g>
<path id="wfig-b-is" d="M80.5 104.5C75.5 101.5 71 102 68.5 105.5C67.5 110 69 114 72 117C77 118 82.5 118.5 86.5 120.5C86 114.5 84 109 80.5 104.5Z" class="wfig-m"></path>
<clipPath id="wfig-bc-is"><use href="#wfig-b-is"></use></clipPath>
<g clip-path="url(#wfig-bc-is)" class="wfig-f"><path d="M87 120L67.5 106M83 110L68 105.2"></path></g>
<path id="wfig-b-tm" d="M87.5 122.5C81 120.5 76 120 72.5 119.5C68.5 118.5 64.5 117.5 61.5 117L61 122.5C67 125 75 128.5 83 131.5C86 129.5 87.5 126 87.5 122.5Z" class="wfig-m" data-unit="teres-major"></path>
<clipPath id="wfig-bc-tm"><use href="#wfig-b-tm"></use></clipPath>
<g clip-path="url(#wfig-bc-tm)" class="wfig-f" data-unit="teres-major"><path d="M88 124.5L60.5 118.6M85 131.5L60.5 122"></path></g>
<path id="wfig-b-la" d="M61 124.5C67.5 127.5 76 131 84 133.5L92.5 134.5C96.5 141 98.8 149 99.8 157C93 163 81 177 73 189C69.5 181.5 68.8 173.5 69.5 168.5C68.5 158.5 65 147 64.5 136C64 130 62.8 127 61 124.5Z" class="wfig-m" data-unit="lats"></path>
<clipPath id="wfig-bc-la"><use href="#wfig-b-la"></use></clipPath>
<g clip-path="url(#wfig-bc-la)" class="wfig-f" data-unit="lats"><path d="M61 123.5L99 142M61 123.5L100 158M61 123.5L90 168M61 123.5L80 180M61 123.5L72 190"></path></g>
<path d="M100.5 156.5C94 162.5 82 176.5 74 188.5C79 188 84.5 189 87.5 190.5C92.5 194 97 198.5 100.5 203.5Z" class="wfig-tendon"></path>
<path id="wfig-b-es" d="M99 158.5C93 166 88.5 176 88.2 186C88.8 194 93.5 200.5 99 204.5Z" class="wfig-m" data-unit="erector-spinae"></path>
<clipPath id="wfig-bc-es"><use href="#wfig-b-es"></use></clipPath>
<g clip-path="url(#wfig-bc-es)" class="wfig-f" data-unit="erector-spinae"><path d="M91 156V206M96 156V206"></path></g>
<path id="wfig-b-ob" d="M69.3 170C68 176 66.5 183 66 190.5L72 189.5C70.5 183.5 69.6 176.5 69.3 170Z" class="wfig-m" data-unit="obliques"></path>
<clipPath id="wfig-bc-ob"><use href="#wfig-b-ob"></use></clipPath>
<g clip-path="url(#wfig-bc-ob)" class="wfig-f" data-unit="obliques"><path d="M69 171L68 191"></path></g>
<path d="M50.2 143L51.6 152L51.2 166.5L47.4 168.5L46.2 164.5L48.8 152Z" class="wfig-tendon"></path>
<path id="wfig-b-tr" d="M58.3 116C60.5 127 59.6 143 56 155C54.6 159.5 53 163 51.8 165.5L51.4 152C51.2 141 51.3 131 51.8 124C53.8 120.5 56 117.5 58.3 116ZM50.2 130C46.2 126.5 44 131 43.6 138C43 148 43.6 157 45.8 163.5L48.2 152C48.8 144 49.4 136 50.2 130Z" class="wfig-m" data-unit="triceps"></path>
<clipPath id="wfig-bc-tr"><use href="#wfig-b-tr"></use></clipPath>
<g clip-path="url(#wfig-bc-tr)" class="wfig-f" data-unit="triceps"><path d="M57.5 113L51.5 167M54 117L51 160M45.5 127L47.4 166"></path></g>
<path id="wfig-b-fa" d="M39.5 171C35 180 33 191 33 202C33 214 33 224 33 233L41.5 233C44.5 222 46.5 210 47.5 198C48.5 188 49.5 180 50.5 173C47 171.5 43 170.5 39.5 171Z" class="wfig-m" data-unit="forearm"></path>
<clipPath id="wfig-bc-fa"><use href="#wfig-b-fa"></use></clipPath>
<g clip-path="url(#wfig-bc-fa)" class="wfig-f" data-unit="forearm"><path d="M40 168L34.5 236M46 168L38 236M52 170L41.5 236"></path></g>
<path id="wfig-b-gm" d="M66.5 192.5C72.5 189 80 188.5 86.5 190.5C84.5 198.5 78.5 204.5 70.5 208.5C67 210 64 212 62 214.5C62 207 63.5 199 66.5 192.5Z" class="wfig-m" data-unit="glute-med"></path>
<clipPath id="wfig-bc-gm"><use href="#wfig-b-gm"></use></clipPath>
<g clip-path="url(#wfig-bc-gm)" class="wfig-f" data-unit="glute-med"><path d="M70 190L62.3 214M80 188L63.5 213.5"></path></g>
<path id="wfig-b-gx" d="M87.5 192C92 195 96.5 198.5 99.5 202.5L99.5 237C94.5 245 84.5 248.5 76.5 246.5C68.5 244.5 62.5 237.5 61.3 227C61 222 61.3 218.5 62.2 216C70.5 210 81 203.5 87.5 192Z" class="wfig-m" data-unit="glute-max"></path>
<clipPath id="wfig-bc-gx"><use href="#wfig-b-gx"></use></clipPath>
<g clip-path="url(#wfig-bc-gx)" class="wfig-f" data-unit="glute-max"><path d="M100 205L62 232.5M100 221L66 245.5M100 237L84 248.5M90 196L60 217.7"></path></g>
<path id="wfig-b-vl" d="M61.2 228C59.6 244 59.8 264 62.8 284C64.2 293 66.2 301 68.2 308L69.6 301C66.6 285 64.6 263 64.4 244Z" class="wfig-m" data-unit="quads"></path>
<clipPath id="wfig-bc-vl"><use href="#wfig-b-vl"></use></clipPath>
<g clip-path="url(#wfig-bc-vl)" class="wfig-f" data-unit="quads"><path d="M62 228L67 306"></path></g>
<path id="wfig-b-bf" d="M65.5 245.5C70 249.5 75 250.5 79.5 249.5C79.5 269 77.5 290 73.5 308.5L70 305C67.5 290 65.6 267 65.5 245.5Z" class="wfig-m" data-unit="hamstrings"></path>
<clipPath id="wfig-bc-bf"><use href="#wfig-b-bf"></use></clipPath>
<g clip-path="url(#wfig-bc-bf)" class="wfig-f" data-unit="hamstrings"><path d="M68 244L71 308M74 247L72.5 308"></path></g>
<path id="wfig-b-st" d="M81 249.5C85.5 249.5 90 247.5 93.5 244C93.8 259 91.5 283 88.5 304.5L85 308.5C83.5 300 81.8 290 81 280C80.8 269 80.9 259 81 249.5Z" class="wfig-m" data-unit="hamstrings"></path>
<clipPath id="wfig-bc-st"><use href="#wfig-b-st"></use></clipPath>
<g clip-path="url(#wfig-bc-st)" class="wfig-f" data-unit="hamstrings"><path d="M84 248L86 308M90 246L87.5 306"></path></g>
<path id="wfig-b-ad" d="M99 236C97.5 242 96 248 95.6 252C94.5 262 93 276 90.6 290L89.8 296C92 282 94.3 262 94.6 246C96 243 97.8 239.5 99 236Z" class="wfig-m" data-unit="adductors"></path>
<clipPath id="wfig-bc-ad"><use href="#wfig-b-ad"></use></clipPath>
<g clip-path="url(#wfig-bc-ad)" class="wfig-f" data-unit="adductors"><path d="M97.5 240L91 292"></path></g>
<path id="wfig-b-ga" d="M66.5 323C63.2 335 63.2 349 67 361C69.5 365 73 365 75.5 362.5L75.5 325C72.5 320.5 69 320.5 66.5 323ZM77.5 325L77.5 364.5C80.5 368.5 84.5 366.5 86.6 362.5C89.6 350.5 89.6 337 87.6 325C84.6 320.5 80.5 320.5 77.5 325Z" class="wfig-m" data-unit="gastrocnemius"></path>
<clipPath id="wfig-bc-ga"><use href="#wfig-b-ga"></use></clipPath>
<g clip-path="url(#wfig-bc-ga)" class="wfig-f" data-unit="gastrocnemius"><path d="M66.5 326L72 364M71 321L74.5 364M82 321L78.6 366M87 326L82 366"></path></g>
<path d="M74 397L80 397L79.2 409L75 409Z" class="wfig-tendon"></path>
<path id="wfig-b-so" d="M65.5 364C66.5 376 69.5 388 73.5 397.5L80.5 397.5C83.5 388 85.5 376 86.5 365.5C83.5 369.5 79.5 370.5 76.5 367.5C73.5 369.5 68.5 367.5 65.5 364Z" class="wfig-m" data-unit="soleus"></path>
<clipPath id="wfig-bc-so"><use href="#wfig-b-so"></use></clipPath>
<g clip-path="url(#wfig-bc-so)" class="wfig-f" data-unit="soleus"><path d="M69 364L74.5 398M76.5 368V398M84 366L79.5 398"></path></g>
<use href="#wfig-bb-sil" clip-path="url(#wfig-bb-sc)" class="wfig-rim"></use>
</g>
</defs>
<use href="#wfig-bf-b"></use>
<use href="#wfig-bf-b" transform="translate(200 0) scale(-1 1)"></use>"##;

#[cfg(test)]
mod tests {
    //! 人体図の色分けの例示テスト。
    //!
    //! 対応する要求: `REQ-LIB-003`、`REQ-LIB-012`。

    use std::collections::BTreeSet;

    use warrior_core::exercise::{
        Attachment, BodyPart, Equipment, Exercise, GripMode, GripSelection, GripStyle, Laterality,
        MuscleAssignment, MuscleRole, MuscleUnit, OtherScope, RecordMode,
    };

    use super::{
        figure_muscles, figure_svg, unit_attribute, FigureMuscles, FigureSide,
        SEATED_ROW_EXERCISE_ID,
    };
    use crate::generated::tokens;
    use crate::grip_options::{attachment_with, initial_grip};

    fn exercise(id: &str, body_part: BodyPart, muscles: Vec<(MuscleUnit, MuscleRole)>) -> Exercise {
        Exercise {
            id: id.to_owned(),
            name: "テスト種目".to_owned(),
            name_kana: "てすとしゅもく".to_owned(),
            body_part,
            equipment: Equipment::Cable,
            record_mode: RecordMode::WeightAndReps,
            laterality: Laterality::Bilateral,
            grip_mode: GripMode::None,
            other_scope: if body_part.is_other() {
                Some(OtherScope::FullBody)
            } else {
                None
            },
            instructions: String::new(),
            cautions: String::new(),
            tips: String::new(),
            sort_order: 0,
            muscles: muscles
                .into_iter()
                .map(|(muscle, role)| MuscleAssignment { muscle, role })
                .collect(),
            grip_options: Vec::new(),
        }
    }

    fn seated_row() -> Exercise {
        let mut value = exercise(
            SEATED_ROW_EXERCISE_ID,
            BodyPart::Back,
            vec![
                (MuscleUnit::LatissimusDorsi, MuscleRole::Primary),
                (MuscleUnit::TrapeziusMiddleLower, MuscleRole::Primary),
                (MuscleUnit::BicepsBrachii, MuscleRole::Secondary),
                (MuscleUnit::PosteriorDeltoid, MuscleRole::Secondary),
            ],
        );
        value.grip_mode = GripMode::Attachment;
        value.grip_options = seated_row_options();
        value
    }

    fn seated_row_options() -> Vec<GripSelection> {
        vec![
            attachment(Attachment::WideBar, Some(GripStyle::Pronated)),
            attachment(Attachment::WideBar, Some(GripStyle::Supinated)),
            attachment(Attachment::StraightBar, Some(GripStyle::Pronated)),
            attachment(Attachment::StraightBar, Some(GripStyle::Supinated)),
            attachment(Attachment::ParallelGrip, None),
            attachment(Attachment::MagNarrowParallel, None),
            attachment(Attachment::MagNarrowOver, None),
            attachment(Attachment::MagNarrowUnder, None),
            attachment(Attachment::MagMediumParallel, None),
            attachment(Attachment::MagMediumOver, None),
            attachment(Attachment::MagMediumUnder, None),
            attachment(Attachment::MagWideOver, None),
        ]
    }

    fn attachment(attachment: Attachment, style: Option<GripStyle>) -> GripSelection {
        GripSelection::Attachment { attachment, style }
    }

    fn units(units: &[MuscleUnit]) -> BTreeSet<MuscleUnit> {
        units.iter().copied().collect()
    }

    #[test]
    fn req_lib_003_the_seated_row_grip_table_changes_the_colors() {
        let seated_row = seated_row();
        let narrow_primary = [MuscleUnit::LatissimusDorsi];
        let narrow_secondary = [MuscleUnit::TrapeziusMiddleLower];
        let wide_primary = [MuscleUnit::TrapeziusUpper, MuscleUnit::TrapeziusMiddleLower];
        let wide_secondary = [MuscleUnit::LatissimusDorsi];
        let other_primary = [
            MuscleUnit::LatissimusDorsi,
            MuscleUnit::TrapeziusMiddleLower,
        ];
        for attachment in [
            Attachment::ParallelGrip,
            Attachment::MagNarrowParallel,
            Attachment::MagNarrowOver,
            Attachment::MagNarrowUnder,
        ] {
            assert_grip_row(
                &seated_row,
                attachment,
                None,
                &narrow_primary,
                &narrow_secondary,
            );
        }
        for attachment in [Attachment::WideBar, Attachment::MagWideOver] {
            assert_grip_row(
                &seated_row,
                attachment,
                Some(GripStyle::Pronated),
                &wide_primary,
                &wide_secondary,
            );
        }
        for attachment in [
            Attachment::StraightBar,
            Attachment::MagMediumParallel,
            Attachment::MagMediumOver,
            Attachment::MagMediumUnder,
        ] {
            assert_grip_row(
                &seated_row,
                attachment,
                Some(GripStyle::Supinated),
                &other_primary,
                &[],
            );
        }
    }

    fn assert_grip_row(
        exercise: &Exercise,
        attachment: Attachment,
        style: Option<GripStyle>,
        primary: &[MuscleUnit],
        secondary: &[MuscleUnit],
    ) {
        assert_grip_selection(
            exercise,
            GripSelection::Attachment { attachment, style },
            primary,
            secondary,
        );
    }

    fn assert_grip_selection(
        exercise: &Exercise,
        selection: GripSelection,
        primary: &[MuscleUnit],
        secondary: &[MuscleUnit],
    ) {
        let resolved = figure_muscles(exercise, Some(selection));
        assert_eq!(units(&resolved.primary), units(primary), "{selection:?}");
        assert_eq!(
            units(&resolved.secondary),
            units(secondary),
            "{selection:?}"
        );
    }

    #[test]
    fn req_lib_003_the_screen_selection_walks_the_seated_row_grip_table() {
        let seated_row = seated_row();
        let options = seated_row.grip_options.clone();
        let mut selection = initial_grip(seated_row.grip_mode, &options)
            .expect("the seated row must have an initial grip");
        assert_grip_selection(
            &seated_row,
            selection,
            &[MuscleUnit::TrapeziusUpper, MuscleUnit::TrapeziusMiddleLower],
            &[MuscleUnit::LatissimusDorsi],
        );

        selection = attachment_with(selection, Attachment::ParallelGrip, &options);
        assert_grip_selection(
            &seated_row,
            selection,
            &[MuscleUnit::LatissimusDorsi],
            &[MuscleUnit::TrapeziusMiddleLower],
        );

        selection = attachment_with(selection, Attachment::StraightBar, &options);
        assert_grip_selection(
            &seated_row,
            selection,
            &[
                MuscleUnit::LatissimusDorsi,
                MuscleUnit::TrapeziusMiddleLower,
            ],
            &[],
        );
    }

    #[test]
    fn req_lib_003_a_combination_outside_the_table_keeps_the_exercise_muscles() {
        let mut lat_pulldown = exercise(
            "a7c4e2f0-0000-4000-8000-00000000001a",
            BodyPart::Back,
            vec![
                (MuscleUnit::LatissimusDorsi, MuscleRole::Primary),
                (MuscleUnit::TeresMajor, MuscleRole::Primary),
                (MuscleUnit::BicepsBrachii, MuscleRole::Secondary),
            ],
        );
        lat_pulldown.grip_mode = GripMode::Attachment;
        lat_pulldown.grip_options = vec![
            attachment(Attachment::WideBar, Some(GripStyle::Pronated)),
            attachment(Attachment::StraightBar, Some(GripStyle::Pronated)),
        ];
        let options = lat_pulldown.grip_options.clone();
        let selection = initial_grip(lat_pulldown.grip_mode, &options)
            .expect("the lat pulldown must have an initial grip");
        let primary = [MuscleUnit::LatissimusDorsi, MuscleUnit::TeresMajor];
        let secondary = [MuscleUnit::BicepsBrachii];
        assert_grip_selection(&lat_pulldown, selection, &primary, &secondary);
        let changed = attachment_with(selection, Attachment::StraightBar, &options);
        assert_grip_selection(&lat_pulldown, changed, &primary, &secondary);

        let seated_row = seated_row();
        let resolved = figure_muscles(&seated_row, None);
        assert_eq!(
            units(&resolved.primary),
            units(&[
                MuscleUnit::LatissimusDorsi,
                MuscleUnit::TrapeziusMiddleLower,
            ])
        );
        assert_eq!(
            units(&resolved.secondary),
            units(&[MuscleUnit::BicepsBrachii, MuscleUnit::PosteriorDeltoid])
        );
    }

    #[test]
    fn req_lib_003_an_exercise_without_grip_options_has_no_initial_grip() {
        let bench = exercise(
            "3f2504e0-4f89-41d3-9a0c-0305e82c3303",
            BodyPart::Chest,
            vec![(MuscleUnit::PectoralisMajorMiddle, MuscleRole::Primary)],
        );
        assert_eq!(initial_grip(bench.grip_mode, &bench.grip_options), None);
    }

    #[test]
    fn req_lib_003_a_custom_exercise_colors_the_whole_body_part() {
        let custom = exercise(
            "3f2504e0-4f89-41d3-9a0c-0305e82c3301",
            BodyPart::Shoulders,
            vec![],
        );
        let resolved = figure_muscles(&custom, None);
        assert_eq!(
            units(&resolved.primary),
            units(&[
                MuscleUnit::AnteriorDeltoid,
                MuscleUnit::LateralDeltoid,
                MuscleUnit::PosteriorDeltoid,
            ])
        );
        assert!(resolved.secondary.is_empty());
    }

    #[test]
    fn req_lib_003_other_exercises_have_no_muscle_colors() {
        let other = exercise(
            "3f2504e0-4f89-41d3-9a0c-0305e82c3302",
            BodyPart::Other,
            vec![],
        );
        let resolved = figure_muscles(&other, None);
        assert!(resolved.primary.is_empty());
        assert!(resolved.secondary.is_empty());
    }

    #[test]
    fn req_lib_012_the_svg_ids_match_the_design_tokens() {
        let mut ids = BTreeSet::new();
        for unit in MuscleUnit::ALL {
            let id = unit_attribute(unit);
            assert_eq!(tokens::muscle_unit_name(id), Some(unit.name_ja()), "{id}");
            assert!(ids.insert(id), "{id}");
        }
        assert_eq!(ids.len(), 24);
        assert_eq!(tokens::MUSCLE_UNITS.len(), 24);
    }

    #[test]
    fn req_lib_012_the_rules_cover_every_unit_and_the_ids_are_unique() {
        let markup = format!(
            "{}{}",
            figure_svg(FigureSide::Front, &FigureMuscles::default()),
            figure_svg(FigureSide::Back, &FigureMuscles::default())
        );
        for unit in MuscleUnit::ALL {
            let id = unit_attribute(unit);
            assert!(markup.contains(&format!(r#".wfig-m[data-unit="{id}"]"#)));
            assert!(markup.contains(&format!(r#".wfig-f[data-unit="{id}"]"#)));
            assert!(markup.contains(&format!(r#".wfig[data-primary~="{id}"]"#)));
            assert!(markup.contains(&format!(r#".wfig[data-secondary~="{id}"]"#)));
        }
        let ids = markup
            .split("id=\"")
            .skip(1)
            .map(|rest| rest.split('"').next().unwrap_or_default())
            .collect::<Vec<_>>();
        assert!(!ids.is_empty());
        for id in &ids {
            assert!(id.starts_with("wfig-"), "{id}");
        }
        let mut unique = ids.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), ids.len());
    }
}

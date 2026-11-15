//! グリップとアタッチメントの選択肢の組み立てと切り替え。
//!
//! 記録画面と種目の詳細が共有する。選択肢は種目スナップショットの `grip_mode` と
//! `grip_options` が持ち、表示の順序を保つ。

use warrior_core::exercise::{Attachment, GripMode, GripSelection, GripStyle, GripWidth};

/// グリップを選ぶ種目の初期の選択。選択肢の先頭を使う。
///
/// グリップを選ばない種目と、選択肢を持たない種目は `None` を返す。
pub fn initial_grip(mode: GripMode, options: &[GripSelection]) -> Option<GripSelection> {
    if mode == GripMode::None {
        return None;
    }
    options.first().copied()
}

/// 選択肢の中の握り方を、選択肢の順に返す。
pub fn grip_styles(options: &[GripSelection]) -> Vec<GripStyle> {
    let mut styles = Vec::new();
    for option in options {
        if let GripSelection::Grip { style, .. } = option {
            if !styles.contains(style) {
                styles.push(*style);
            }
        }
    }
    styles
}

/// 指定した握り方で選べる手幅を、選択肢の順に返す。
pub fn grip_widths(options: &[GripSelection], style: GripStyle) -> Vec<GripWidth> {
    let mut widths = Vec::new();
    for option in options {
        if let GripSelection::Grip {
            style: option_style,
            width,
        } = option
        {
            if *option_style == style && !widths.contains(width) {
                widths.push(*width);
            }
        }
    }
    widths
}

/// 選択肢の中のアタッチメントを、選択肢の順に返す。
pub fn grip_attachments(options: &[GripSelection]) -> Vec<Attachment> {
    let mut attachments = Vec::new();
    for option in options {
        if let GripSelection::Attachment { attachment, .. } = option {
            if !attachments.contains(attachment) {
                attachments.push(*attachment);
            }
        }
    }
    attachments
}

/// 指定したアタッチメントで選べる握り方を、選択肢の順に返す。
pub fn attachment_styles(options: &[GripSelection], attachment: Attachment) -> Vec<GripStyle> {
    let mut styles = Vec::new();
    for option in options {
        if let GripSelection::Attachment {
            attachment: option_attachment,
            style: Some(style),
        } = option
        {
            if *option_attachment == attachment && !styles.contains(style) {
                styles.push(*style);
            }
        }
    }
    styles
}

/// 握り方を変えた選択を返す。同じ手幅が無ければ、その握り方の先頭にする。
pub fn grip_with_style(
    selection: GripSelection,
    style: GripStyle,
    options: &[GripSelection],
) -> GripSelection {
    let width = match selection {
        GripSelection::Grip { width, .. } => Some(width),
        _ => None,
    };
    width
        .and_then(|width| {
            options.iter().copied().find(|option| {
                matches!(
                    option,
                    GripSelection::Grip {
                        style: option_style,
                        width: option_width,
                    } if *option_style == style && *option_width == width
                )
            })
        })
        .or_else(|| {
            options.iter().copied().find(|option| {
                matches!(
                    option,
                    GripSelection::Grip {
                        style: option_style,
                        ..
                    } if *option_style == style
                )
            })
        })
        .unwrap_or(selection)
}

/// 手幅を変えた選択を返す。同じ握り方が無ければ、その手幅の先頭にする。
pub fn grip_with_width(
    selection: GripSelection,
    width: GripWidth,
    options: &[GripSelection],
) -> GripSelection {
    let style = match selection {
        GripSelection::Grip { style, .. } => Some(style),
        _ => None,
    };
    style
        .and_then(|style| {
            options.iter().copied().find(|option| {
                matches!(
                    option,
                    GripSelection::Grip {
                        style: option_style,
                        width: option_width,
                    } if *option_style == style && *option_width == width
                )
            })
        })
        .or_else(|| {
            options.iter().copied().find(|option| {
                matches!(
                    option,
                    GripSelection::Grip {
                        width: option_width,
                        ..
                    } if *option_width == width
                )
            })
        })
        .unwrap_or(selection)
}

/// アタッチメントを変えた選択を返す。同じ握り方が無ければ、そのアタッチメントの先頭にする。
pub fn attachment_with(
    selection: GripSelection,
    attachment: Attachment,
    options: &[GripSelection],
) -> GripSelection {
    let style = match selection {
        GripSelection::Attachment { style, .. } => style,
        _ => None,
    };
    options
        .iter()
        .copied()
        .find(|option| {
            matches!(
                option,
                GripSelection::Attachment {
                    attachment: option_attachment,
                    style: option_style,
                } if *option_attachment == attachment && *option_style == style
            )
        })
        .or_else(|| {
            options.iter().copied().find(|option| {
                matches!(
                    option,
                    GripSelection::Attachment {
                        attachment: option_attachment,
                        ..
                    } if *option_attachment == attachment
                )
            })
        })
        .unwrap_or(selection)
}

/// アタッチメントの握り方を変えた選択を返す。
pub fn attachment_with_style(
    selection: GripSelection,
    style: GripStyle,
    options: &[GripSelection],
) -> GripSelection {
    let attachment = match selection {
        GripSelection::Attachment { attachment, .. } => Some(attachment),
        _ => None,
    };
    attachment
        .and_then(|attachment| {
            options.iter().copied().find(|option| {
                matches!(
                    option,
                    GripSelection::Attachment {
                        attachment: option_attachment,
                        style: Some(option_style),
                    } if *option_attachment == attachment && *option_style == style
                )
            })
        })
        .unwrap_or(selection)
}

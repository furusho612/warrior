//! グリップとアタッチメントの表示文言と、保存されたキーの読み取り。
//!
//! 記録画面、カレンダー、サマリーが共有する。表示文言は `Calendar*` のキーを使う。

use warrior_core::exercise::{Attachment, GripStyle, GripWidth};

use crate::text::{text, Key};

/// 保存されたキーから、握り方と手幅の表示文言を作る。
pub(crate) fn grip_label(key: &str) -> String {
    let (style, width) = key.split_once('_').unwrap_or((key, ""));
    let style = grip_style_from_key(style).map_or_else(
        || style.to_owned(),
        |style| grip_style_text(style).to_owned(),
    );
    let width = if width.is_empty() {
        None
    } else {
        Some(grip_width_from_key(width).map_or_else(
            || width.to_owned(),
            |width| grip_width_text(width).to_owned(),
        ))
    };
    match width {
        Some(width) => format!("{style} {width}"),
        None => style,
    }
}

/// 保存されたキーから、アタッチメントの表示文言を作る。
pub(crate) fn attachment_label(key: &str) -> String {
    attachment_from_key(key).map_or_else(
        || key.to_owned(),
        |attachment| attachment_text(attachment).to_owned(),
    )
}

/// 握り方の表示文言。
pub(crate) fn grip_style_text(style: GripStyle) -> &'static str {
    match style {
        GripStyle::Pronated => text(Key::CalendarGripPronated),
        GripStyle::Supinated => text(Key::CalendarGripSupinated),
        GripStyle::Parallel => text(Key::CalendarGripParallel),
    }
}

/// 手幅の表示文言。
pub(crate) fn grip_width_text(width: GripWidth) -> &'static str {
    match width {
        GripWidth::Wide => text(Key::CalendarGripWide),
        GripWidth::Medium => text(Key::CalendarGripMedium),
        GripWidth::Narrow => text(Key::CalendarGripNarrow),
    }
}

/// アタッチメントの表示文言。
pub(crate) fn attachment_text(attachment: Attachment) -> &'static str {
    match attachment {
        Attachment::WideBar => text(Key::CalendarAttachmentWideBar),
        Attachment::StraightBar => text(Key::CalendarAttachmentStraightBar),
        Attachment::ParallelGrip => text(Key::CalendarAttachmentParallelGrip),
        Attachment::MagNarrowParallel => text(Key::CalendarAttachmentMagNarrowParallel),
        Attachment::MagNarrowOver => text(Key::CalendarAttachmentMagNarrowOver),
        Attachment::MagNarrowUnder => text(Key::CalendarAttachmentMagNarrowUnder),
        Attachment::MagMediumParallel => text(Key::CalendarAttachmentMagMediumParallel),
        Attachment::MagMediumOver => text(Key::CalendarAttachmentMagMediumOver),
        Attachment::MagMediumUnder => text(Key::CalendarAttachmentMagMediumUnder),
        Attachment::MagWideOver => text(Key::CalendarAttachmentMagWideOver),
    }
}

/// 保存されたキーから握り方を読む。
pub(crate) fn grip_style_from_key(key: &str) -> Option<GripStyle> {
    GripStyle::ALL
        .iter()
        .copied()
        .find(|style| style.as_str() == key)
}

/// 保存されたキーから手幅を読む。
pub(crate) fn grip_width_from_key(key: &str) -> Option<GripWidth> {
    GripWidth::ALL
        .iter()
        .copied()
        .find(|width| width.as_str() == key)
}

/// 保存されたキーからアタッチメントを読む。
pub(crate) fn attachment_from_key(key: &str) -> Option<Attachment> {
    Attachment::ALL
        .iter()
        .copied()
        .find(|attachment| attachment.as_str() == key)
}

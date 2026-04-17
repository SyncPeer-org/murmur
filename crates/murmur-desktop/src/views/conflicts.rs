//! Conflicts list and file history views.

use iced::widget::{button, column, container, row, scrollable, text};
use iced::{Color, Element, Length};

use crate::app::{App, ConflictDiffCache, Screen};
use crate::helpers::{format_size, truncate_hex};
use crate::message::Message;
use crate::style::*;

impl App {
    pub(crate) fn view_conflicts(&self) -> Element<'_, Message> {
        let mut col = column![text("Conflicts").size(24).color(Color::WHITE),].spacing(16);
        if self.conflicts.is_empty() {
            col = col.push(
                container(text("No active conflicts.").size(14).color(TEXT_MUTED))
                    .padding(20)
                    .width(Length::Fill)
                    .style(card_style),
            );
        } else {
            // Bulk actions per folder
            let mut folder_ids: Vec<String> =
                self.conflicts.iter().map(|c| c.folder_id.clone()).collect();
            folder_ids.dedup();
            for fid in &folder_ids {
                let fname = self
                    .conflicts
                    .iter()
                    .find(|c| c.folder_id == *fid)
                    .map(|c| c.folder_name.as_str())
                    .unwrap_or("unknown");
                col = col.push(
                    row![
                        text(format!("{fname}:"))
                            .size(16)
                            .color(Color::WHITE)
                            .width(Length::Fill),
                        button(text("Keep All Newest").size(13))
                            .on_press(Message::BulkResolve {
                                folder_id: fid.clone(),
                                strategy: "keep_newest".to_string()
                            })
                            .style(primary_btn)
                            .padding(6),
                    ]
                    .spacing(8)
                    .align_y(iced::Alignment::Center),
                );
            }
            // Individual conflicts
            for conflict in &self.conflicts {
                let mut card_content = column![
                    text(format!("{} / {}", conflict.folder_name, conflict.path))
                        .size(14)
                        .color(Color::WHITE),
                ]
                .spacing(6);
                for v in &conflict.versions {
                    card_content = card_content.push(
                        row![
                            text(format!(
                                "{} ({})  HLC: {}",
                                v.device_name,
                                truncate_hex(&v.blob_hash, 16),
                                v.hlc
                            ))
                            .size(12)
                            .color(TEXT_SECONDARY)
                            .width(Length::Fill),
                            button(text("Keep").size(12))
                                .on_press(Message::ResolveConflict {
                                    folder_id: conflict.folder_id.clone(),
                                    path: conflict.path.clone(),
                                    chosen_hash: v.blob_hash.clone()
                                })
                                .style(primary_btn)
                                .padding(4),
                        ]
                        .spacing(4)
                        .align_y(iced::Alignment::Center),
                    );
                }
                let key = (conflict.folder_id.clone(), conflict.path.clone());
                let expanded = self.expanded_conflict_diffs.contains(&key);
                let diff_label = if expanded { "Hide diff" } else { "Show diff" };
                card_content = card_content.push(
                    row![
                        button(text(diff_label).size(12))
                            .on_press(Message::ToggleConflictDiff {
                                folder_id: conflict.folder_id.clone(),
                                path: conflict.path.clone(),
                            })
                            .style(secondary_btn)
                            .padding(6),
                        button(text("Keep Both (dismiss)").size(12))
                            .on_press(Message::DismissConflict {
                                folder_id: conflict.folder_id.clone(),
                                path: conflict.path.clone(),
                            })
                            .style(secondary_btn)
                            .padding(6),
                    ]
                    .spacing(6),
                );
                if expanded {
                    card_content = card_content
                        .push(render_conflict_diff_panel(self.conflict_diffs.get(&key)));
                }
                col = col.push(
                    container(card_content)
                        .padding(14)
                        .width(Length::Fill)
                        .style(card_style),
                );
            }
        }
        col.into()
    }

    pub(crate) fn view_file_history(&self) -> Element<'_, Message> {
        let mut content = column![
            text(format!("History: {}", self.history_path))
                .size(14)
                .color(Color::WHITE)
        ]
        .spacing(8);
        if self.history_versions.is_empty() {
            content = content.push(text("No versions found.").size(14).color(TEXT_MUTED));
        } else {
            for v in &self.history_versions {
                content = content.push(
                    row![
                        text(format!(
                            "{}  by {}  HLC: {}  ({})",
                            truncate_hex(&v.blob_hash, 16),
                            v.device_name,
                            v.modified_at,
                            format_size(v.size)
                        ))
                        .size(13)
                        .color(TEXT_SECONDARY)
                        .width(Length::Fill),
                        button(text("Restore").size(12))
                            .on_press(Message::RestoreVersion {
                                folder_id: self.history_folder_id.clone(),
                                path: self.history_path.clone(),
                                blob_hash: v.blob_hash.clone()
                            })
                            .style(primary_btn)
                            .padding(6),
                    ]
                    .spacing(8)
                    .align_y(iced::Alignment::Center),
                );
            }
        }

        column![
            row![
                button(text("Back").size(13))
                    .on_press(Message::Navigate(Screen::Folders))
                    .style(secondary_btn)
                    .padding(iced::Padding {
                        top: 6.0,
                        right: 12.0,
                        bottom: 6.0,
                        left: 12.0,
                    }),
                text("File History").size(24).color(Color::WHITE),
            ]
            .spacing(12)
            .align_y(iced::Alignment::Center),
            container(content)
                .padding(16)
                .width(Length::Fill)
                .style(card_style),
        ]
        .spacing(16)
        .into()
    }
}

/// Build the inline diff panel for a single conflict (M29).
///
/// `diff` is `None` while the IPC fetch is in flight, and `Some(cache)` once
/// the daemon has responded. Binary pairs render a single summary line; text
/// pairs render a unified diff via `similar::TextDiff`.
fn render_conflict_diff_panel(diff: Option<&ConflictDiffCache>) -> Element<'_, Message> {
    let Some(cache) = diff else {
        return container(text("Loading diff…").size(12).color(TEXT_MUTED))
            .padding(8)
            .into();
    };

    let header = text(format!(
        "{} ({} bytes) vs {} ({} bytes)",
        cache.left.device_name, cache.left.size, cache.right.device_name, cache.right.size
    ))
    .size(12)
    .color(TEXT_MUTED);

    if !cache.is_text {
        return column![
            header,
            text(format!(
                "binary files differ — {} vs {} bytes",
                cache.left.size, cache.right.size
            ))
            .size(12)
            .color(TEXT_SECONDARY),
        ]
        .spacing(4)
        .into();
    }

    let left_text = String::from_utf8_lossy(&cache.left.bytes);
    let right_text = String::from_utf8_lossy(&cache.right.bytes);
    let diff_text = format_unified_diff(&left_text, &right_text);

    column![
        header,
        container(scrollable(text(diff_text).size(12).color(Color::WHITE)))
            .padding(6)
            .width(Length::Fill)
            .max_height(300.0)
            .style(card_style),
    ]
    .spacing(4)
    .into()
}

/// Render a unified diff between two strings at line granularity.
fn format_unified_diff(left: &str, right: &str) -> String {
    use similar::{ChangeTag, TextDiff};
    let diff = TextDiff::from_lines(left, right);
    let mut out = String::new();
    for change in diff.iter_all_changes() {
        let sign = match change.tag() {
            ChangeTag::Delete => "-",
            ChangeTag::Insert => "+",
            ChangeTag::Equal => " ",
        };
        out.push_str(sign);
        out.push_str(change.value());
        if !change.value().ends_with('\n') {
            out.push('\n');
        }
    }
    out
}

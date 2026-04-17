//! Devices screen view.

use iced::widget::{button, column, container, row, text};
use iced::{Color, Element, Length};

use crate::app::App;
use crate::helpers::{format_relative_time, truncate_hex};
use crate::message::Message;
use crate::style::*;

impl App {
    pub(crate) fn view_devices(&self) -> Element<'_, Message> {
        let mut col = column![
            row![
                text("Devices")
                    .size(24)
                    .color(Color::WHITE)
                    .width(Length::Fill),
                button(text("Invite device").size(13))
                    .on_press(Message::IssuePairingInvite)
                    .style(primary_btn)
                    .padding(iced::Padding {
                        top: 6.0,
                        right: 16.0,
                        bottom: 6.0,
                        left: 16.0,
                    }),
            ]
            .align_y(iced::Alignment::Center),
        ]
        .spacing(16);

        // Pairing invite card.
        if let Some(invite) = &self.pairing_invite {
            let expiry_note = format!(
                "Valid until UNIX {} — share this URL with the joining device.",
                invite.expires_at_unix
            );
            let card = container(
                column![
                    text("Pairing Invite").size(14).color(TEXT_SECONDARY),
                    text(&invite.url).size(12).color(Color::WHITE),
                    text(expiry_note).size(11).color(TEXT_MUTED),
                    row![
                        button(text("Copy URL").size(12))
                            .on_press(Message::CopyPairingInviteUrl)
                            .style(primary_btn)
                            .padding(6),
                        button(text("Dismiss").size(12))
                            .on_press(Message::ClearPairingInvite)
                            .style(secondary_btn)
                            .padding(6),
                    ]
                    .spacing(6),
                ]
                .spacing(6),
            )
            .padding(14)
            .width(Length::Fill)
            .style(card_style);
            col = col.push(card);
        }

        // This device
        if let Some(local) = self
            .devices
            .iter()
            .find(|d| d.device_id == self.status_device_id)
        {
            let card = container(
                column![
                    text("This Device").size(14).color(TEXT_MUTED),
                    row![
                        text("\u{2022}").size(12).color(ACCENT),
                        text(&local.name)
                            .size(15)
                            .color(Color::WHITE)
                            .width(Length::Fill),
                        text("Online").size(12).color(ACCENT),
                    ]
                    .spacing(8)
                    .align_y(iced::Alignment::Center),
                    text(truncate_hex(&local.device_id, 16))
                        .size(11)
                        .color(TEXT_MUTED),
                ]
                .spacing(6),
            )
            .padding(14)
            .width(Length::Fill)
            .style(card_style);
            col = col.push(card);
        }

        // Pending approval
        if !self.pending.is_empty() {
            let mut section =
                column![text("Pending Approval").size(14).color(TEXT_SECONDARY)].spacing(6);
            for d in &self.pending {
                section = section.push(
                    row![
                        text(format!("{} ({})", d.name, truncate_hex(&d.device_id, 16)))
                            .size(14)
                            .color(Color::WHITE)
                            .width(Length::Fill),
                        button(text("Approve").size(13))
                            .on_press(Message::ApproveDevice(d.device_id.clone()))
                            .style(primary_btn)
                            .padding(6),
                    ]
                    .spacing(8)
                    .align_y(iced::Alignment::Center),
                );
            }
            col = col.push(
                container(section)
                    .padding(14)
                    .width(Length::Fill)
                    .style(card_style),
            );
        }

        // Other devices
        let others: Vec<_> = self
            .devices
            .iter()
            .filter(|d| d.device_id != self.status_device_id)
            .collect();
        if !others.is_empty() {
            let mut section =
                column![text("Other Devices").size(14).color(TEXT_SECONDARY)].spacing(6);
            for d in others {
                let presence = self
                    .device_presence
                    .iter()
                    .find(|p| p.device_id == d.device_id);
                let (status, color) = match presence {
                    Some(p) if p.online => ("Online".to_string(), ACCENT),
                    Some(p) if p.last_seen_unix > 0 => {
                        (format_relative_time(p.last_seen_unix), TEXT_MUTED)
                    }
                    _ => ("Never connected".to_string(), TEXT_MUTED),
                };
                section = section.push(
                    row![
                        text("\u{2022}").size(12).color(color),
                        text(&d.name)
                            .size(14)
                            .color(Color::WHITE)
                            .width(Length::Fill),
                        text(status).size(12).color(color),
                    ]
                    .spacing(8)
                    .align_y(iced::Alignment::Center),
                );
            }
            col = col.push(
                container(section)
                    .padding(14)
                    .width(Length::Fill)
                    .style(card_style),
            );
        } else if self.devices.len() <= 1 {
            col = col.push(
                container(
                    text("No other devices on this network.")
                        .size(14)
                        .color(TEXT_MUTED),
                )
                .padding(14)
                .width(Length::Fill)
                .style(card_style),
            );
        }
        col.into()
    }
}

# Murmur — Implementation Plan

## How to Use This Document

This is the **implementation plan** for remaining Murmur milestones. Work milestone by milestone, in order.
For each milestone: implement, test, `cargo clippy -- -D warnings`, `cargo fmt`, stop.

For architecture and design details, see [architecture.md](architecture.md).
For a feature overview, see [features.md](features.md).

## Current Status (as of 2026-03-29)

| Milestone                                                        | Status     |
| ---------------------------------------------------------------- | ---------- |
| 0–17 — MVP : DAG, Network, Engine, Daemon, FFI, Android, Desktop | ✅ Done    |
| 18 — Desktop App: IPC Refactor & Core Screens                    | ✅ Done    |
| 19 — Zero-Config Onboarding & Default Folder                     | ✅ Done    |
| 20 — System Tray & Notifications (IPC: PauseSync/ResumeSync)     | ✅ Done    |
| 21 — Folder Discovery & Selective Sync                           | ✅ Done    |
| 22 — Rich Conflict Resolution                                    | ✅ Done    |
| 23 — Device Management Improvements                              | ✅ Done    |
| 24 — Sync Progress, Pause/Resume & Bandwidth                     | ✅ Done    |
| 25 — File Browser & Search                                       | ✅ Done    |
| 26 — Settings & Configuration UI                                 | ✅ Done    |
| 27 — Diagnostics & Network Health                                | ✅ Done    |
| 28 — Web Dashboard (htmx)                                        | 🔲 Planned |

---

## Phase 2: Desktop app remaining Features

System Tray & Notifications

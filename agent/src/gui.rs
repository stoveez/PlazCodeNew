use crate::{mcp_addons, preferences};
use eframe::{egui, App, Frame, NativeOptions};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

const BG: egui::Color32 = egui::Color32::from_rgb(6, 20, 38);
const SIDEBAR: egui::Color32 = egui::Color32::from_rgb(7, 24, 44);
const PANEL: egui::Color32 = egui::Color32::from_rgb(9, 27, 49);
const PANEL_HI: egui::Color32 = egui::Color32::from_rgb(16, 43, 73);
const TERM: egui::Color32 = egui::Color32::from_rgb(4, 16, 30);
const LINE: egui::Color32 = egui::Color32::from_rgb(105, 86, 46);
const FG: egui::Color32 = egui::Color32::from_rgb(238, 244, 255);
const DIM: egui::Color32 = egui::Color32::from_rgb(167, 184, 204);
const FAINT: egui::Color32 = egui::Color32::from_rgb(113, 133, 158);
const ACCENT: egui::Color32 = egui::Color32::from_rgb(217, 173, 82);
const ACCENT_HI: egui::Color32 = egui::Color32::from_rgb(240, 207, 122);
const INK: egui::Color32 = egui::Color32::from_rgb(7, 20, 38);
const GREEN: egui::Color32 = egui::Color32::from_rgb(88, 207, 139);
const RED: egui::Color32 = egui::Color32::from_rgb(235, 101, 96);
const AMBER: egui::Color32 = egui::Color32::from_rgb(240, 180, 90);
const GREY: egui::Color32 = egui::Color32::from_rgb(78, 91, 112);

pub struct UiShared {
    pub studio_running: AtomicBool,
    pub mcp_alive: Arc<AtomicBool>,
    pub workspace_ready: AtomicBool,
    pub full_access: std::sync::RwLock<Arc<AtomicBool>>,
    pub workspace_root: Mutex<String>,
    pub fatal: Mutex<Option<String>>,
    pub logs: Mutex<VecDeque<String>>,
    pub extension_seen_ms: std::sync::atomic::AtomicU64,
    pub show_requested: AtomicBool,
    pub quit_requested: AtomicBool,
    pub ui_context: Mutex<Option<egui::Context>>,
    pub tools: Mutex<Vec<String>>,
    pub servers: Mutex<Vec<mcp_addons::ServerSummary>>,
}

const LOG_CAP: usize = 1200;

impl UiShared {
    pub fn new(mcp_alive: Arc<AtomicBool>) -> Self {
        Self {
            studio_running: AtomicBool::new(false),
            mcp_alive,
            workspace_ready: AtomicBool::new(false),
            full_access: std::sync::RwLock::new(Arc::new(AtomicBool::new(false))),
            workspace_root: Mutex::new(String::new()),
            fatal: Mutex::new(None),
            logs: Mutex::new(VecDeque::with_capacity(LOG_CAP)),
            extension_seen_ms: std::sync::atomic::AtomicU64::new(0),
            show_requested: AtomicBool::new(false),
            quit_requested: AtomicBool::new(false),
            ui_context: Mutex::new(None),
            tools: Mutex::new(Vec::new()),
            servers: Mutex::new(Vec::new()),
        }
    }

    pub fn attach_workspace(&self, root: String, full_flag: Arc<AtomicBool>) {
        if let Ok(mut r) = self.workspace_root.lock() {
            *r = root;
        }
        self.workspace_ready.store(true, Ordering::Relaxed);
        let current = self.full_access.read()
            .map(|f| f.load(Ordering::Relaxed))
            .unwrap_or(false);
        full_flag.store(current, Ordering::Relaxed);
        if let Ok(mut slot) = self.full_access.write() {
            *slot = full_flag;
        }
    }

    pub fn log(&self, chunk: &str) {
        let mut logs = self.logs.lock().unwrap_or_else(|e| e.into_inner());
        for line in chunk.lines() {
            let trimmed = line.trim_end();
            if trimmed.is_empty() {
                continue;
            }
            if logs.len() == LOG_CAP {
                logs.pop_front();
            }
            logs.push_back(trimmed.to_string());
        }
    }

    pub fn clear_logs(&self) {
        if let Ok(mut logs) = self.logs.lock() {
            logs.clear();
        }
    }

    pub fn mark_extension_seen(&self) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        self.extension_seen_ms.store(now, Ordering::Relaxed);
    }

    pub fn extension_recent(&self) -> bool {
        let seen = self.extension_seen_ms.load(Ordering::Relaxed);
        if seen == 0 {
            return false;
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        now.saturating_sub(seen) < 15_000
    }

    pub fn register_context(&self, ctx: &egui::Context) {
        if let Ok(mut slot) = self.ui_context.lock() {
            if slot.is_none() {
                *slot = Some(ctx.clone());
            }
        }
    }

    pub fn request_show(&self) {
        self.show_requested.store(true, Ordering::Relaxed);
        if let Ok(slot) = self.ui_context.lock() {
            if let Some(ctx) = slot.as_ref() {
                ctx.request_repaint();
            }
        }
    }

    pub fn request_quit(&self) {
        self.quit_requested.store(true, Ordering::Relaxed);
        if let Ok(slot) = self.ui_context.lock() {
            if let Some(ctx) = slot.as_ref() {
                ctx.request_repaint();
            }
        }
    }

    pub fn set_fatal(&self, msg: String) {
        let mut fatal = self.fatal.lock().unwrap_or_else(|e| e.into_inner());
        if fatal.is_none() {
            *fatal = Some(msg);
        }
    }

    pub fn set_tool_snapshot(&self, tools: &[serde_json::Value], servers: Vec<mcp_addons::ServerSummary>) {
        if let Ok(mut names) = self.tools.lock() {
            *names = tools.iter()
                .filter_map(|tool| tool.get("name").and_then(|v| v.as_str()).map(str::to_string))
                .collect();
            names.sort();
        }
        if let Ok(mut current) = self.servers.lock() {
            *current = servers;
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Home,
    Tools,
    Mcp,
    Console,
    Settings,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Level {
    Info,
    Warn,
    Err,
}

fn display_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

fn line_level(line: &str) -> Level {
    let head = &line[..line.len().min(100)];
    if head.contains("ERROR") {
        Level::Err
    } else if head.contains("WARN") {
        Level::Warn
    } else {
        Level::Info
    }
}

fn level_color(level: Level) -> egui::Color32 {
    match level {
        Level::Err => RED,
        Level::Warn => AMBER,
        Level::Info => egui::Color32::from_rgb(192, 204, 222),
    }
}

pub struct AgentApp {
    shared: Arc<UiShared>,
    restart_tx: tokio::sync::mpsc::UnboundedSender<()>,
    preferences: Arc<preferences::PreferencesStore>,
    booted: Instant,
    page: Page,
    log_filter: usize,
    tool_filter: String,
    catalog_notice: Option<(String, bool)>,
}

impl AgentApp {
    fn dot(ui: &mut egui::Ui, color: egui::Color32) {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(11.0, 11.0), egui::Sense::hover());
        ui.painter().circle_filled(rect.center(), 3.6, color);
        ui.painter().circle_stroke(rect.center(), 4.8, egui::Stroke::new(1.0, color.gamma_multiply(0.45)));
    }

    fn panel() -> egui::Frame {
        egui::Frame::none()
            .fill(PANEL)
            .stroke(egui::Stroke::new(1.0, LINE))
            .rounding(egui::Rounding::same(10.0))
            .inner_margin(egui::Margin::same(14.0))
    }

    fn section_title(ui: &mut egui::Ui, title: &str, subtitle: &str) {
        ui.label(egui::RichText::new(title).size(22.0).strong().color(FG));
        ui.add_space(2.0);
        ui.label(egui::RichText::new(subtitle).size(11.0).color(FAINT));
        ui.add_space(14.0);
    }

    fn nav_button(&mut self, ui: &mut egui::Ui, page: Page, icon: &str, label: &str) {
        let selected = self.page == page;
        let text = egui::RichText::new(format!("{icon}  {label}"))
            .size(12.0)
            .strong()
            .color(if selected { INK } else { DIM });
        let button = egui::Button::new(text)
            .fill(if selected { ACCENT } else { egui::Color32::TRANSPARENT })
            .stroke(if selected {
                egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(ACCENT.r(), ACCENT.g(), ACCENT.b(), 120))
            } else {
                egui::Stroke::NONE
            })
            .rounding(egui::Rounding::same(7.0))
            .min_size(egui::vec2(ui.available_width(), 34.0));
        if ui.add(button).clicked() {
            self.page = page;
        }
    }

    fn render_sidebar(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("plazcode-sidebar")
            .resizable(false)
            .exact_width(178.0)
            .frame(egui::Frame::none().fill(SIDEBAR).inner_margin(egui::Margin::same(12.0)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(28.0, 28.0), egui::Sense::hover());
                    ui.painter().rect_filled(rect, egui::Rounding::same(6.0), ACCENT);
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "P",
                        egui::FontId::proportional(16.0),
                        INK,
                    );
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new("PlazCode").size(14.0).strong().color(FG));
                        ui.label(egui::RichText::new(format!("v{}", display_version())).size(9.5).color(FAINT));
                    });
                });

                ui.add_space(20.0);
                self.nav_button(ui, Page::Home, "⌂", "Home");
                self.nav_button(ui, Page::Tools, "⌘", "Tools");
                self.nav_button(ui, Page::Mcp, "◫", "MCP Servers");
                self.nav_button(ui, Page::Console, ">_", "Console");
                self.nav_button(ui, Page::Settings, "⚙", "Settings");

                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    let studio = self.shared.studio_running.load(Ordering::Relaxed);
                    let mcp = self.shared.mcp_alive.load(Ordering::Relaxed);
                    ui.horizontal(|ui| {
                        Self::dot(ui, if studio && mcp { GREEN } else { GREY });
                        ui.label(egui::RichText::new(if studio && mcp { "Studio connected" } else { "Studio offline" }).size(10.5).color(DIM));
                    });
                    ui.add_space(8.0);
                });
            });
    }

    fn render_header(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(
                    egui::Button::new(egui::RichText::new("Hide").size(10.5).strong().color(DIM))
                        .fill(PANEL_HI)
                        .stroke(egui::Stroke::new(1.0, LINE))
                        .rounding(egui::Rounding::same(7.0))
                ).clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Visible(false));
                    self.shared.log("desktop window hidden — PlazCode is still running in the background");
                }
                if ui.add(
                    egui::Button::new(egui::RichText::new("Restart MCP").size(10.5).strong().color(INK))
                        .fill(ACCENT)
                        .stroke(egui::Stroke::new(1.0, ACCENT_HI))
                        .rounding(egui::Rounding::same(7.0))
                ).clicked() {
                    let _ = self.restart_tx.send(());
                }
                let up = self.booted.elapsed();
                let text = if up.as_secs() >= 3600 {
                    format!("up {}h {:02}m", up.as_secs() / 3600, (up.as_secs() % 3600) / 60)
                } else {
                    format!("up {}m {:02}s", up.as_secs() / 60, up.as_secs() % 60)
                };
                ui.label(egui::RichText::new(text).size(10.0).color(FAINT));
            });
        });

        if let Some(msg) = self.shared.fatal.lock().unwrap_or_else(|e| e.into_inner()).clone() {
            ui.add_space(8.0);
            egui::Frame::none()
                .fill(egui::Color32::from_rgba_unmultiplied(RED.r(), RED.g(), RED.b(), 28))
                .stroke(egui::Stroke::new(1.0, RED))
                .rounding(egui::Rounding::same(8.0))
                .inner_margin(egui::Margin::same(9.0))
                .show(ui, |ui| {
                    ui.label(egui::RichText::new(format!("Agent error: {msg}")).size(11.0).color(RED));
                });
        }
        ui.add_space(6.0);
    }

    fn stat_card(ui: &mut egui::Ui, title: &str, value: &str, subtitle: &str, ok: bool) {
        Self::panel().show(ui, |ui| {
            ui.set_min_width(180.0);
            ui.horizontal(|ui| {
                Self::dot(ui, if ok { GREEN } else { GREY });
                ui.label(egui::RichText::new(title).size(10.0).strong().color(DIM));
            });
            ui.add_space(9.0);
            ui.label(egui::RichText::new(value).size(18.0).strong().color(FG));
            ui.label(egui::RichText::new(subtitle).size(9.5).color(FAINT));
        });
    }

    fn render_home(&mut self, ui: &mut egui::Ui) {
        Self::section_title(ui, "Control Center", "PlazCode extension, Roblox Studio bridge and local workspace status.");

        let studio = self.shared.studio_running.load(Ordering::Relaxed);
        let mcp = self.shared.mcp_alive.load(Ordering::Relaxed);
        let workspace = self.shared.workspace_ready.load(Ordering::Relaxed);
        let extension = self.shared.extension_recent();
        let tools = self.shared.tools.lock().map(|v| v.len()).unwrap_or(0);
        let root = self.shared.workspace_root.lock().map(|v| v.clone()).unwrap_or_default();

        ui.columns(4, |cols| {
            Self::stat_card(&mut cols[0], "Browser extension", if extension { "Connected" } else { "Waiting" }, "Last contact < 15 seconds", extension);
            Self::stat_card(&mut cols[1], "Roblox Studio", if studio && mcp { "Connected" } else if studio { "Open" } else { "Offline" }, "Built-in Studio MCP", studio && mcp);
            Self::stat_card(&mut cols[2], "AgentScript", if workspace { "Ready" } else { "Offline" }, "Files + terminal workspace", workspace);
            Self::stat_card(&mut cols[3], "Available tools", &tools.to_string(), "Studio + configured MCP", tools > 0);
        });

        ui.add_space(12.0);
        Self::panel().show(ui, |ui| {
            ui.label(egui::RichText::new("Workspace").size(12.0).strong().color(FG));
            ui.add_space(7.0);
            ui.label(egui::RichText::new(if root.is_empty() { "(not configured)" } else { &root }).monospace().size(10.5).color(DIM));
            ui.add_space(9.0);

            let prefs = self.preferences.snapshot();
            ui.horizontal_wrapped(|ui| {
                badge(ui, "Engine", if prefs.engine == "local" { "AgentScript" } else { "Roblox" });
                badge(ui, "Work mode", &prefs.work_mode);
                badge(ui, "Reasoning", &prefs.thinking_level);
                badge(ui, "Plan", if prefs.plan_mode { "on" } else { "off" });
                badge(ui, "Extra", if prefs.extra_thinking { "on" } else { "off" });
            });
        });

        ui.add_space(12.0);
        Self::panel().show(ui, |ui| {
            ui.label(egui::RichText::new("Regression-safe architecture").size(12.0).strong().color(FG));
            ui.add_space(6.0);
            ui.label(egui::RichText::new("The desktop app controls the existing local agent and shared settings. Browser providers keep their existing adapters and message protocol.").size(10.5).color(DIM));
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                badge(ui, "Extension relay", "unchanged contract");
                badge(ui, "Provider DOM", "isolated");
                badge(ui, "Pairing", "required");
                badge(ui, "Loopback", "127.0.0.1");
            });
        });
    }

    fn render_tools(&mut self, ui: &mut egui::Ui) {
        Self::section_title(ui, "Tools", "Live tools discovered from Roblox Studio and enabled MCP add-ons.");

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Filter").size(10.0).color(FAINT));
            ui.add(
                egui::TextEdit::singleline(&mut self.tool_filter)
                    .hint_text("search tools")
                    .desired_width(260.0)
            );
        });
        ui.add_space(10.0);

        let tools = self.shared.tools.lock().map(|v| v.clone()).unwrap_or_default();
        let needle = self.tool_filter.trim().to_ascii_lowercase();
        let filtered: Vec<String> = tools.into_iter()
            .filter(|name| needle.is_empty() || name.to_ascii_lowercase().contains(&needle))
            .collect();

        Self::panel().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(format!("{} tools", filtered.len())).size(10.0).strong().color(ACCENT_HI));
                if filtered.is_empty() {
                    ui.label(egui::RichText::new("Connect Studio or enable an MCP server to populate this list.").size(9.5).color(FAINT));
                }
            });
            ui.add_space(8.0);
            egui::ScrollArea::vertical().id_salt("tools-list").max_height(510.0).show(ui, |ui| {
                for name in filtered {
                    let addon = name.contains("__");
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(if addon { "MCP" } else { "RS" }).size(8.5).strong().color(if addon { ACCENT_HI } else { GREEN }));
                        ui.label(egui::RichText::new(name).monospace().size(10.5).color(FG));
                    });
                    ui.add_space(3.0);
                }
            });
        });
    }

    fn render_mcp(&mut self, ui: &mut egui::Ui) {
        Self::section_title(ui, "MCP Servers", "Enable optional MCP servers without changing browser provider code.");

        if let Some((message, ok)) = &self.catalog_notice {
            let color = if *ok { GREEN } else { RED };
            egui::Frame::none()
                .fill(egui::Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 20))
                .stroke(egui::Stroke::new(1.0, color))
                .rounding(egui::Rounding::same(7.0))
                .inner_margin(egui::Margin::same(8.0))
                .show(ui, |ui| {
                    ui.label(egui::RichText::new(message).size(10.0).color(color));
                });
            ui.add_space(10.0);
        }

        let live = self.shared.servers.lock().map(|v| v.clone()).unwrap_or_default();
        for entry in mcp_addons::catalog() {
            let enabled = mcp_addons::is_enabled(entry.id);
            let status = live.iter().find(|server| server.id == entry.id);

            Self::panel().show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new(entry.name).size(12.0).strong().color(FG));
                        ui.label(egui::RichText::new(entry.description).size(9.5).color(FAINT));
                        ui.add_space(5.0);
                        ui.label(egui::RichText::new(format!("{} {}", entry.command, entry.args.join(" "))).monospace().size(9.0).color(DIM));
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let label = if enabled { "Disable" } else { "Enable" };
                        if ui.add(
                            egui::Button::new(egui::RichText::new(label).size(10.0).strong().color(if enabled { RED } else { FG }))
                                .fill(PANEL_HI)
                                .stroke(egui::Stroke::new(1.0, if enabled { RED.gamma_multiply(0.6) } else { LINE }))
                                .rounding(egui::Rounding::same(6.0))
                        ).clicked() {
                            match mcp_addons::set_catalog_enabled(entry.id, !enabled) {
                                Ok(()) => {
                                    self.catalog_notice = Some((format!("{} {}. Tool discovery refreshes on the next bridge tool-list request.", entry.name, if enabled { "disabled" } else { "enabled" }), true));
                                }
                                Err(error) => {
                                    self.catalog_notice = Some((error.to_string(), false));
                                }
                            }
                        }

                        if let Some(server) = status {
                            ui.label(egui::RichText::new(if server.alive { format!("{} tools", server.tools) } else { "start failed".to_string() }).size(9.0).color(if server.alive { GREEN } else { AMBER }));
                        } else if enabled {
                            ui.label(egui::RichText::new("configured").size(9.0).color(ACCENT_HI));
                        }
                    });
                });
            });
            ui.add_space(8.0);
        }

        Self::panel().show(ui, |ui| {
            ui.label(egui::RichText::new("Roblox Studio MCP").size(12.0).strong().color(FG));
            ui.label(egui::RichText::new("Primary server. It is always enabled and cannot be removed from PlazCode.").size(9.5).color(FAINT));
        });
    }

    fn render_console(&mut self, ui: &mut egui::Ui) {
        Self::section_title(ui, "Console", "Agent, Studio MCP and local bridge logs.");

        ui.horizontal(|ui| {
            for (idx, label) in ["All", "Info", "Warn", "Error"].iter().enumerate() {
                if ui.selectable_label(self.log_filter == idx, *label).clicked() {
                    self.log_filter = idx;
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Clear").clicked() {
                    self.shared.clear_logs();
                }
            });
        });
        ui.add_space(8.0);

        let logs = self.shared.logs.lock()
            .map(|v| v.iter().cloned().collect::<Vec<_>>())
            .unwrap_or_default();

        egui::Frame::none()
            .fill(TERM)
            .stroke(egui::Stroke::new(1.0, LINE))
            .rounding(egui::Rounding::same(9.0))
            .inner_margin(egui::Margin::same(10.0))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("console-scroll")
                    .stick_to_bottom(true)
                    .max_height(560.0)
                    .show(ui, |ui| {
                        for line in logs {
                            let level = line_level(&line);
                            let visible = match self.log_filter {
                                1 => level == Level::Info,
                                2 => level == Level::Warn,
                                3 => level == Level::Err,
                                _ => true,
                            };
                            if visible {
                                ui.label(egui::RichText::new(line).monospace().size(9.6).color(level_color(level)));
                            }
                        }
                    });
            });
    }

    fn render_settings(&mut self, ui: &mut egui::Ui) {
        Self::section_title(ui, "Settings", "These values sync with the extension and apply to supported AI tabs.");

        let mut prefs = self.preferences.snapshot();
        let mut changed = false;

        Self::panel().show(ui, |ui| {
            ui.label(egui::RichText::new("Engine").size(11.0).strong().color(FG));
            ui.label(egui::RichText::new("Choose where PlazCode commands run.").size(9.5).color(FAINT));
            ui.add_space(7.0);
            ui.horizontal(|ui| {
                changed |= choice(ui, &mut prefs.engine, "roblox", "Roblox Studio");
                changed |= choice(ui, &mut prefs.engine, "local", "AgentScript");
            });
        });

        ui.add_space(9.0);
        Self::panel().show(ui, |ui| {
            ui.label(egui::RichText::new("Work mode").size(11.0).strong().color(FG));
            ui.label(egui::RichText::new("Matches the extension's Fast, Balanced and Thorough presets.").size(9.5).color(FAINT));
            ui.add_space(7.0);
            let before = prefs.work_mode.clone();
            ui.horizontal(|ui| {
                let _ = choice(ui, &mut prefs.work_mode, "fast", "Fast");
                let _ = choice(ui, &mut prefs.work_mode, "balanced", "Balanced");
                let _ = choice(ui, &mut prefs.work_mode, "thorough", "Thorough");
            });
            if prefs.work_mode != before {
                changed = true;
                match prefs.work_mode.as_str() {
                    "fast" => {
                        prefs.thinking_level = "low".to_string();
                        prefs.extra_thinking = false;
                    }
                    "thorough" => {
                        prefs.thinking_level = "high".to_string();
                        prefs.extra_thinking = true;
                    }
                    _ => {
                        prefs.thinking_level = "mid".to_string();
                        prefs.extra_thinking = false;
                    }
                }
            }
        });

        ui.add_space(9.0);
        Self::panel().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new("Reasoning level").size(11.0).strong().color(FG));
                    ui.label(egui::RichText::new("Preference passed into the existing provider workflow.").size(9.5).color(FAINT));
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let old = prefs.thinking_level.clone();
                    egui::ComboBox::from_id_salt("reasoning-level")
                        .selected_text(reasoning_label(&prefs.thinking_level))
                        .show_ui(ui, |ui| {
                            for (value, label) in [
                                ("default", "Default"),
                                ("low", "Low"),
                                ("mid", "Medium"),
                                ("high", "High"),
                                ("max", "Max"),
                            ] {
                                ui.selectable_value(&mut prefs.thinking_level, value.to_string(), label);
                            }
                        });
                    if prefs.thinking_level != old {
                        changed = true;
                    }
                });
            });
        });

        ui.add_space(9.0);
        Self::panel().show(ui, |ui| {
            changed |= toggle_row(ui, "Extra Thinking", "Allow longer reasoning behavior where the provider supports it.", &mut prefs.extra_thinking);
            ui.separator();
            changed |= toggle_row(ui, "Plan Mode", "Ask the agent to plan before making project changes.", &mut prefs.plan_mode);
            ui.separator();
            changed |= toggle_row(ui, "Forge UI", "Keep PlazCode's UI-building helper behavior enabled.", &mut prefs.forge_mode);
            ui.separator();
            changed |= toggle_row(ui, "Auto Fix", "Automatically react to supported playtest errors.", &mut prefs.auto_fix);
            ui.separator();
            changed |= toggle_row(ui, "Background Mode", "Allow the agent loop to continue while the tab is not focused.", &mut prefs.bg_mode);
            ui.separator();
            changed |= toggle_row(ui, "Sounds", "Play PlazCode UI feedback sounds.", &mut prefs.sounds);
        });

        ui.add_space(9.0);
        Self::panel().show(ui, |ui| {
            ui.label(egui::RichText::new("AgentScript permissions").size(11.0).strong().color(FG));
            ui.label(egui::RichText::new("Full access expands the local workspace security boundary.").size(9.5).color(FAINT));
            ui.add_space(7.0);
            let before = prefs.perm_mode.clone();
            ui.horizontal(|ui| {
                let _ = choice(ui, &mut prefs.perm_mode, "sandbox", "Sandbox");
                let _ = choice(ui, &mut prefs.perm_mode, "ask", "Ask");
                let _ = choice(ui, &mut prefs.perm_mode, "full", "Full");
            });
            if prefs.perm_mode != before {
                changed = true;
            }
        });

        if changed {
            let full = prefs.perm_mode == "full";
            match self.preferences.replace(prefs) {
                Ok(_) => {
                    if let Ok(flag) = self.shared.full_access.read() {
                        flag.store(full, Ordering::Relaxed);
                    }
                }
                Err(error) => {
                    self.shared.log(&format!("ERROR desktop settings save failed: {error:#}"));
                }
            }
        }

        ui.add_space(9.0);
        Self::panel().show(ui, |ui| {
            ui.label(egui::RichText::new("Desktop app").size(11.0).strong().color(FG));
            ui.label(egui::RichText::new("Closing the window keeps PlazCode and the bridge running in the background. Launch PlazCode.exe again to reopen it.").size(9.5).color(FAINT));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.add(
                    egui::Button::new(egui::RichText::new("Hide to background").size(10.0).strong().color(INK))
                        .fill(ACCENT)
                        .stroke(egui::Stroke::new(1.0, ACCENT_HI))
                        .rounding(egui::Rounding::same(6.0))
                ).clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Visible(false));
                    self.shared.log("desktop window hidden — PlazCode is still running in the background");
                }
                if ui.add(
                    egui::Button::new(egui::RichText::new("Exit PlazCode").size(10.0).strong().color(RED))
                        .fill(PANEL_HI)
                        .stroke(egui::Stroke::new(1.0, RED.gamma_multiply(0.6)))
                        .rounding(egui::Rounding::same(6.0))
                ).clicked() {
                    self.shared.request_quit();
                }
            });
        });

        ui.add_space(10.0);
        ui.label(egui::RichText::new(format!("Saved locally to {}", self.preferences.path().display())).size(9.0).color(FAINT));
    }
}

impl App for AgentApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut Frame) {
        self.shared.register_context(ctx);
        ctx.request_repaint_after(std::time::Duration::from_millis(500));

        if self.shared.show_requested.swap(false, Ordering::Relaxed) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }

        if self.shared.quit_requested.load(Ordering::Relaxed) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        if ctx.input(|input| input.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            self.shared.log("desktop window hidden — PlazCode is still running in the background");
            return;
        }

        self.render_sidebar(ctx);

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(BG).inner_margin(egui::Margin::symmetric(18.0, 14.0)))
            .show(ctx, |ui| {
                self.render_header(ui);
                match self.page {
                    Page::Home => self.render_home(ui),
                    Page::Tools => self.render_tools(ui),
                    Page::Mcp => self.render_mcp(ui),
                    Page::Console => self.render_console(ui),
                    Page::Settings => self.render_settings(ui),
                }
            });
    }
}

fn choice(ui: &mut egui::Ui, current: &mut String, value: &str, label: &str) -> bool {
    let selected = current == value;
    let clicked = ui.add(
        egui::Button::new(egui::RichText::new(label).size(10.0).strong().color(if selected { INK } else { DIM }))
            .fill(if selected { ACCENT } else { PANEL_HI })
            .stroke(egui::Stroke::new(1.0, if selected { ACCENT_HI } else { LINE }))
            .rounding(egui::Rounding::same(6.0))
    ).clicked();
    if clicked && !selected {
        *current = value.to_string();
        return true;
    }
    false
}

fn toggle_row(ui: &mut egui::Ui, title: &str, subtitle: &str, value: &mut bool) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(egui::RichText::new(title).size(10.5).strong().color(FG));
            ui.label(egui::RichText::new(subtitle).size(9.0).color(FAINT));
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.add(
                egui::Button::new(egui::RichText::new(if *value { "ON" } else { "OFF" }).size(9.5).strong().color(if *value { INK } else { DIM }))
                    .fill(if *value { ACCENT } else { PANEL_HI })
                    .stroke(egui::Stroke::new(1.0, if *value { ACCENT_HI } else { LINE }))
                    .rounding(egui::Rounding::same(10.0))
                    .min_size(egui::vec2(48.0, 24.0))
            ).clicked() {
                *value = !*value;
                changed = true;
            }
        });
    });
    changed
}

fn badge(ui: &mut egui::Ui, key: &str, value: &str) {
    egui::Frame::none()
        .fill(PANEL_HI)
        .stroke(egui::Stroke::new(1.0, LINE))
        .rounding(egui::Rounding::same(6.0))
        .inner_margin(egui::Margin::symmetric(7.0, 4.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(key).size(8.5).color(FAINT));
                ui.label(egui::RichText::new(value).size(8.5).strong().color(FG));
            });
        });
}

fn reasoning_label(value: &str) -> &'static str {
    match value {
        "low" => "Low",
        "mid" => "Medium",
        "high" => "High",
        "max" => "Max",
        _ => "Default",
    }
}

fn window_icon() -> Arc<egui::IconData> {
    let width = 64usize;
    let height = 64usize;
    let mut rgba = vec![0u8; width * height * 4];
    for y in 0..height {
        for x in 0..width {
            let i = (y * width + x) * 4;
            let dx = x as i32 - 32;
            let dy = y as i32 - 32;
            if dx * dx + dy * dy < 28 * 28 {
                rgba[i..i + 4].copy_from_slice(&[6, 20, 38, 255]);
            }
        }
    }
    for y in 16..48 {
        for x in 18..46 {
            if x < 23 || x > 40 || y < 21 || y > 42 {
                let i = (y * width + x) * 4;
                rgba[i..i + 4].copy_from_slice(&[217, 173, 82, 255]);
            }
        }
    }
    Arc::new(egui::IconData { rgba, width: width as u32, height: height as u32 })
}

pub fn run_gui(
    shared: Arc<UiShared>,
    restart_tx: tokio::sync::mpsc::UnboundedSender<()>,
    preferences: Arc<preferences::PreferencesStore>,
) -> eframe::Result<()> {
    let options = NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1040.0, 700.0])
            .with_min_inner_size([760.0, 520.0])
            .with_icon(window_icon())
            .with_title("PlazCode"),
        ..Default::default()
    };

    eframe::run_native(
        "PlazCode",
        options,
        Box::new(move |_cc| {
            Ok(Box::new(AgentApp {
                shared,
                restart_tx,
                preferences,
                booted: Instant::now(),
                page: Page::Home,
                log_filter: 0,
                tool_filter: String::new(),
                catalog_notice: None,
            }))
        }),
    )
}

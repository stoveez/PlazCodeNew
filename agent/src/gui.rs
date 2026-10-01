use crate::{mcp_addons, preferences};
use eframe::{egui, App, Frame, NativeOptions};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

const BG: egui::Color32 = egui::Color32::from_rgb(3, 10, 20);
const SIDEBAR: egui::Color32 = egui::Color32::from_rgb(4, 13, 26);
const PANEL: egui::Color32 = egui::Color32::from_rgb(8, 18, 33);
const PANEL_HI: egui::Color32 = egui::Color32::from_rgb(13, 29, 50);
const TERM: egui::Color32 = egui::Color32::from_rgb(2, 8, 16);
const LINE: egui::Color32 = egui::Color32::from_rgb(35, 52, 73);
const FG: egui::Color32 = egui::Color32::from_rgb(245, 248, 255);
const DIM: egui::Color32 = egui::Color32::from_rgb(177, 194, 218);
const FAINT: egui::Color32 = egui::Color32::from_rgb(104, 126, 154);
const ACCENT: egui::Color32 = egui::Color32::from_rgb(255, 145, 38);
const ACCENT_HI: egui::Color32 = egui::Color32::from_rgb(255, 193, 82);
const INK: egui::Color32 = egui::Color32::from_rgb(6, 13, 24);
const GREEN: egui::Color32 = egui::Color32::from_rgb(49, 224, 164);
const RED: egui::Color32 = egui::Color32::from_rgb(255, 105, 108);
const AMBER: egui::Color32 = egui::Color32::from_rgb(255, 179, 63);
const GREY: egui::Color32 = egui::Color32::from_rgb(74, 89, 112);
const BLUE: egui::Color32 = egui::Color32::from_rgb(75, 160, 255);
const PURPLE: egui::Color32 = egui::Color32::from_rgb(160, 106, 255);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolDisplay {
    pub name: String,
    pub source: String,
    pub available: bool,
}

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
    pub tools: Mutex<Vec<ToolDisplay>>,
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

    fn replace_source_tools(&self, source: &str, tools: &[serde_json::Value], available: bool) {
        if let Ok(mut rows) = self.tools.lock() {
            rows.retain(|row| row.source != source);
            rows.extend(tools.iter().filter_map(|tool| {
                tool.get("name").and_then(|value| value.as_str()).map(|name| ToolDisplay {
                    name: name.to_string(),
                    source: source.to_string(),
                    available,
                })
            }));
            rows.sort_by(|a, b| a.source.cmp(&b.source).then_with(|| a.name.cmp(&b.name)));
            rows.dedup_by(|a, b| a.source == b.source && a.name == b.name);
        }
    }

    pub fn set_roblox_tools(&self, tools: &[serde_json::Value], available: bool) {
        self.replace_source_tools("Roblox Studio", tools, available);
    }

    pub fn set_local_tools(&self, tools: &[serde_json::Value], available: bool) {
        self.replace_source_tools("AgentScript", tools, available);
    }

    pub fn set_addon_tools(&self, tools: &[serde_json::Value], servers: Vec<mcp_addons::ServerSummary>) {
        if let Ok(mut rows) = self.tools.lock() {
            rows.retain(|row| !row.source.starts_with("MCP / "));
            for tool in tools {
                let Some(name) = tool.get("name").and_then(|value| value.as_str()) else { continue; };
                let server = tool.get("server").and_then(|value| value.as_str())
                    .or_else(|| name.split_once("__").map(|parts| parts.0))
                    .unwrap_or("addon");
                rows.push(ToolDisplay {
                    name: name.to_string(),
                    source: format!("MCP / {server}"),
                    available: true,
                });
            }
            rows.sort_by(|a, b| a.source.cmp(&b.source).then_with(|| a.name.cmp(&b.name)));
            rows.dedup_by(|a, b| a.source == b.source && a.name == b.name);
        }
        if let Ok(mut current) = self.servers.lock() {
            *current = servers;
        }
    }

    pub fn set_browser_tools(&self, tools: &[serde_json::Value]) {
        if let Ok(mut rows) = self.tools.lock() {
            rows.retain(|row| !row.source.starts_with("PlazCode"));
            for tool in tools {
                let Some(name) = tool.get("name").and_then(|value| value.as_str()) else { continue; };
                let source = tool.get("source").and_then(|value| value.as_str()).unwrap_or("PlazCode");
                rows.push(ToolDisplay {
                    name: name.to_string(),
                    source: source.to_string(),
                    available: true,
                });
            }
            rows.sort_by(|a, b| a.source.cmp(&b.source).then_with(|| a.name.cmp(&b.name)));
            rows.dedup_by(|a, b| a.source == b.source && a.name == b.name);
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

fn install_style(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(9.0, 9.0);
    style.spacing.button_padding = egui::vec2(13.0, 8.0);
    style.visuals.panel_fill = BG;
    style.visuals.window_fill = PANEL;
    style.visuals.faint_bg_color = PANEL_HI;
    style.visuals.extreme_bg_color = TERM;
    style.visuals.selection.bg_fill = ACCENT;
    style.visuals.selection.stroke = egui::Stroke::new(1.0, ACCENT_HI);
    style.visuals.hyperlink_color = ACCENT_HI;
    ctx.set_style(style);
}

pub struct AgentApp {
    shared: Arc<UiShared>,
    restart_tx: tokio::sync::mpsc::UnboundedSender<()>,
    preferences: Arc<preferences::PreferencesStore>,
    booted: Instant,
    page: Page,
    log_filter: usize,
    tool_filter: String,
    tool_source: usize,
    catalog_notice: Option<(String, bool)>,
}

impl AgentApp {
    fn dot(ui: &mut egui::Ui, color: egui::Color32) {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
        ui.painter().circle_filled(rect.center(), 3.8, color);
        ui.painter().circle_stroke(rect.center(), 5.1, egui::Stroke::new(1.0, color.gamma_multiply(0.35)));
    }

    fn panel() -> egui::Frame {
        egui::Frame::none()
            .fill(PANEL)
            .stroke(egui::Stroke::new(1.0, LINE))
            .rounding(egui::Rounding::same(12.0))
            .inner_margin(egui::Margin::same(16.0))
    }

    fn glow_panel(color: egui::Color32) -> egui::Frame {
        egui::Frame::none()
            .fill(PANEL)
            .stroke(egui::Stroke::new(1.0, color.gamma_multiply(0.58)))
            .rounding(egui::Rounding::same(12.0))
            .inner_margin(egui::Margin::same(16.0))
    }

    fn section_title(ui: &mut egui::Ui, title: &str, subtitle: &str) {
        ui.label(egui::RichText::new(title).size(29.0).strong().color(FG));
        ui.add_space(3.0);
        ui.label(egui::RichText::new(subtitle).size(11.5).color(DIM));
        ui.add_space(15.0);
    }

    fn glow_outline(ui: &egui::Ui, rect: egui::Rect, color: egui::Color32, strong: bool) {
        let levels = if strong {
            [(0.5, 120u8), (2.5, 48u8), (5.0, 18u8)]
        } else {
            [(0.5, 78u8), (2.0, 28u8), (4.0, 10u8)]
        };
        for (inset, alpha) in levels {
            let inner = rect.shrink(inset);
            if inner.width() <= 2.0 || inner.height() <= 2.0 {
                continue;
            }
            ui.painter().rect_stroke(
                inner,
                egui::Rounding::same(11.0),
                egui::Stroke::new(
                    1.0,
                    egui::Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha),
                ),
            );
        }
    }

    fn paint_background(ui: &egui::Ui) {
        let rect = ui.max_rect();
        ui.painter().rect_filled(rect, egui::Rounding::ZERO, BG);
    }

    fn paint_logo(ui: &egui::Ui, rect: egui::Rect, color: egui::Color32) {
        let painter = ui.painter();
        let w = rect.width();
        let h = rect.height();
        let stroke = egui::Stroke::new((w * 0.07).max(4.0), color);
        let x = rect.left();
        let y = rect.top();

        painter.line_segment(
            [egui::pos2(x + w * 0.23, y + h * 0.18), egui::pos2(x + w * 0.23, y + h * 0.62)],
            stroke,
        );
        painter.line_segment(
            [egui::pos2(x + w * 0.23, y + h * 0.18), egui::pos2(x + w * 0.58, y + h * 0.18)],
            stroke,
        );
        painter.line_segment(
            [egui::pos2(x + w * 0.23, y + h * 0.42), egui::pos2(x + w * 0.56, y + h * 0.42)],
            stroke,
        );
        painter.circle_stroke(
            egui::pos2(x + w * 0.57, y + h * 0.30),
            w * 0.15,
            stroke,
        );

        let lower = egui::Stroke::new((w * 0.055).max(3.0), color);
        painter.line_segment(
            [egui::pos2(x + w * 0.27, y + h * 0.68), egui::pos2(x + w * 0.15, y + h * 0.91)],
            lower,
        );
        painter.line_segment(
            [egui::pos2(x + w * 0.42, y + h * 0.68), egui::pos2(x + w * 0.30, y + h * 0.91)],
            lower,
        );
    }

    fn paint_nav_icon(ui: &egui::Ui, rect: egui::Rect, page: Page, color: egui::Color32) {
        let painter = ui.painter();
        let c = rect.center();
        let s = rect.width().min(rect.height()) * 0.28;
        let stroke = egui::Stroke::new(1.8, color);

        match page {
            Page::Home => {
                painter.line_segment([egui::pos2(c.x - s, c.y), egui::pos2(c.x, c.y - s)], stroke);
                painter.line_segment([egui::pos2(c.x, c.y - s), egui::pos2(c.x + s, c.y)], stroke);
                painter.rect_stroke(
                    egui::Rect::from_min_max(
                        egui::pos2(c.x - s * 0.72, c.y),
                        egui::pos2(c.x + s * 0.72, c.y + s * 0.78),
                    ),
                    egui::Rounding::same(2.0),
                    stroke,
                );
            }
            Page::Tools => {
                painter.line_segment(
                    [egui::pos2(c.x - s * 0.82, c.y + s * 0.82), egui::pos2(c.x + s * 0.72, c.y - s * 0.72)],
                    stroke,
                );
                painter.circle_stroke(egui::pos2(c.x + s * 0.55, c.y - s * 0.55), s * 0.43, stroke);
                painter.circle_filled(egui::pos2(c.x - s * 0.72, c.y + s * 0.72), 2.4, color);
            }
            Page::Mcp => {
                for off in [-0.55f32, 0.0, 0.55] {
                    let yy = c.y + off * s;
                    painter.line_segment([egui::pos2(c.x - s, yy), egui::pos2(c.x + s, yy)], stroke);
                    painter.circle_filled(egui::pos2(c.x - s, yy), 2.0, color);
                    painter.circle_filled(egui::pos2(c.x + s, yy), 2.0, color);
                }
            }
            Page::Console => {
                painter.rect_stroke(rect.shrink(4.0), egui::Rounding::same(3.0), stroke);
                painter.line_segment(
                    [egui::pos2(c.x - s * 0.55, c.y - s * 0.35), egui::pos2(c.x - s * 0.05, c.y)],
                    stroke,
                );
                painter.line_segment(
                    [egui::pos2(c.x - s * 0.05, c.y), egui::pos2(c.x - s * 0.55, c.y + s * 0.35)],
                    stroke,
                );
                painter.line_segment(
                    [egui::pos2(c.x + s * 0.10, c.y + s * 0.42), egui::pos2(c.x + s * 0.65, c.y + s * 0.42)],
                    stroke,
                );
            }
            Page::Settings => {
                painter.circle_stroke(c, s * 0.60, stroke);
                painter.circle_stroke(c, s * 0.22, stroke);
                for angle in [0.0f32, 1.5708, 3.14159, 4.71239] {
                    let dx = angle.cos() * s;
                    let dy = angle.sin() * s;
                    painter.line_segment(
                        [egui::pos2(c.x + dx * 0.67, c.y + dy * 0.67), egui::pos2(c.x + dx, c.y + dy)],
                        stroke,
                    );
                }
            }
        }
    }

    fn nav_button(&mut self, ui: &mut egui::Ui, page: Page, label: &str) {
        let selected = self.page == page;
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), 45.0),
            egui::Sense::click(),
        );
        let hovered = response.hovered();

        if selected || hovered {
            ui.painter().rect_filled(
                rect,
                egui::Rounding::same(9.0),
                if selected {
                    egui::Color32::from_rgba_unmultiplied(ACCENT.r(), ACCENT.g(), ACCENT.b(), 34)
                } else {
                    egui::Color32::from_rgba_unmultiplied(255, 255, 255, 8)
                },
            );
        }
        if selected {
            ui.painter().rect_filled(
                egui::Rect::from_min_max(
                    egui::pos2(rect.left(), rect.top() + 4.0),
                    egui::pos2(rect.left() + 3.0, rect.bottom() - 4.0),
                ),
                egui::Rounding::same(2.0),
                ACCENT,
            );
            Self::glow_outline(ui, rect, ACCENT, true);
        }

        let icon_rect = egui::Rect::from_center_size(
            egui::pos2(rect.left() + 24.0, rect.center().y),
            egui::vec2(25.0, 25.0),
        );
        Self::paint_nav_icon(ui, icon_rect, page, if selected { ACCENT_HI } else { DIM });
        ui.painter().text(
            egui::pos2(rect.left() + 47.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(12.5),
            if selected { FG } else { DIM },
        );

        if response.clicked() {
            self.page = page;
        }
    }

    fn render_sidebar(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("plazcode-sidebar")
            .resizable(false)
            .exact_width(202.0)
            .frame(
                egui::Frame::none()
                    .fill(SIDEBAR)
                    .inner_margin(egui::Margin::same(14.0))
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(24, 39, 58)))
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(44.0, 44.0), egui::Sense::hover());
                    for (expand, alpha) in [(1.0, 80u8), (4.0, 30u8), (8.0, 10u8)] {
                        ui.painter().rect_stroke(
                            rect.expand(expand),
                            egui::Rounding::same(11.0),
                            egui::Stroke::new(
                                1.0,
                                egui::Color32::from_rgba_unmultiplied(ACCENT.r(), ACCENT.g(), ACCENT.b(), alpha),
                            ),
                        );
                    }
                    ui.painter().rect_filled(rect, egui::Rounding::same(11.0), PANEL_HI);
                    Self::paint_logo(ui, rect.shrink(7.0), ACCENT_HI);
                    ui.vertical(|ui| {
                        ui.add_space(3.0);
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            ui.label(egui::RichText::new("Plaz").size(17.5).strong().color(FG));
                            ui.label(egui::RichText::new("Code").size(17.5).strong().color(ACCENT_HI));
                        });
                        ui.label(egui::RichText::new(format!("v{}", display_version())).size(8.8).color(FAINT));
                    });
                });

                ui.add_space(22.0);
                self.nav_button(ui, Page::Home, "Home");
                self.nav_button(ui, Page::Tools, "Tools");
                self.nav_button(ui, Page::Mcp, "MCP Servers");
                self.nav_button(ui, Page::Console, "Terminal");
                self.nav_button(ui, Page::Settings, "Settings");

                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    let studio = self.shared.studio_running.load(Ordering::Relaxed);
                    let mcp = self.shared.mcp_alive.load(Ordering::Relaxed);
                    let extension = self.shared.extension_recent();

                    Self::glow_panel(if extension { GREEN } else { AMBER }).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            Self::dot(ui, if extension { GREEN } else { AMBER });
                            ui.vertical(|ui| {
                                ui.label(egui::RichText::new(if extension { "Bridge connected" } else { "Waiting for extension" }).size(10.5).strong().color(FG));
                                ui.label(egui::RichText::new("127.0.0.1 local bridge").size(8.7).color(FAINT));
                            });
                        });
                    });

                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        Self::dot(ui, if studio && mcp { GREEN } else { GREY });
                        ui.label(
                            egui::RichText::new(if studio && mcp { "Studio connected" } else { "Studio offline" })
                                .size(9.5)
                                .color(DIM)
                        );
                    });
                    ui.add_space(4.0);
                });
            });
    }

    fn render_header(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("BUILD MORE  /  CODE SMARTER  /  WITH AI")
                    .size(8.8)
                    .color(FAINT)
            );

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(
                    egui::Button::new(egui::RichText::new("Hide").size(10.0).strong().color(DIM))
                        .fill(PANEL_HI)
                        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(42, 58, 78)))
                        .rounding(egui::Rounding::same(8.0))
                ).clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Visible(false));
                    self.shared.log("desktop window hidden — PlazCode is still running in the background");
                }

                if ui.add(
                    egui::Button::new(egui::RichText::new("Restart MCP").size(10.0).strong().color(INK))
                        .fill(ACCENT)
                        .stroke(egui::Stroke::new(1.0, ACCENT_HI))
                        .rounding(egui::Rounding::same(8.0))
                ).clicked() {
                    let _ = self.restart_tx.send(());
                }

                let up = self.booted.elapsed();
                let uptime = if up.as_secs() >= 3600 {
                    format!("up {}h {:02}m", up.as_secs() / 3600, (up.as_secs() % 3600) / 60)
                } else {
                    format!("up {}m {:02}s", up.as_secs() / 60, up.as_secs() % 60)
                };
                ui.label(egui::RichText::new(uptime).size(9.2).color(FAINT));
            });
        });

        if let Some(msg) = self.shared.fatal.lock().unwrap_or_else(|e| e.into_inner()).clone() {
            ui.add_space(9.0);
            egui::Frame::none()
                .fill(egui::Color32::from_rgba_unmultiplied(RED.r(), RED.g(), RED.b(), 24))
                .stroke(egui::Stroke::new(1.0, RED.gamma_multiply(0.8)))
                .rounding(egui::Rounding::same(9.0))
                .inner_margin(egui::Margin::same(10.0))
                .show(ui, |ui| {
                    ui.label(egui::RichText::new(format!("Agent error: {msg}")).size(10.5).color(RED));
                });
        }
        ui.add_space(8.0);
    }

    fn hero(&self, ui: &mut egui::Ui) {
        let width = ui.available_width();
        let wide = width >= 860.0;
        let shown = egui::Frame::none()
            .fill(egui::Color32::from_rgb(6, 15, 28))
            .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(255, 145, 38, 92)))
            .rounding(egui::Rounding::same(15.0))
            .inner_margin(egui::Margin::symmetric(22.0, 18.0))
            .show(ui, |ui| {
                ui.set_min_height(if wide { 142.0 } else { 126.0 });

                let render_copy = |ui: &mut egui::Ui| {
                    ui.add_space(3.0);
                    ui.label(egui::RichText::new("Welcome to").size(if wide { 21.0 } else { 18.0 }).strong().color(FG));
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        ui.label(egui::RichText::new("Plaz").size(if wide { 38.0 } else { 32.0 }).strong().color(FG));
                        ui.label(egui::RichText::new("Code").size(if wide { 38.0 } else { 32.0 }).strong().color(ACCENT_HI));
                    });
                    ui.add_space(1.0);
                    ui.label(
                        egui::RichText::new("Your browser-first AI development control center for Roblox.")
                            .size(11.0)
                            .color(DIM),
                    );
                    ui.add_space(10.0);
                    ui.horizontal_wrapped(|ui| {
                        badge(ui, "Browser AI", if self.shared.extension_recent() { "connected" } else { "waiting" });
                        badge(ui, "Bridge", "127.0.0.1");
                        badge(ui, "Desktop", "background ready");
                    });
                };

                if wide {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| render_copy(ui));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let (logo_rect, _) = ui.allocate_exact_size(egui::vec2(132.0, 96.0), egui::Sense::hover());
                            for (radius, alpha) in [(57.0, 6u8), (40.0, 9u8), (25.0, 12u8)] {
                                ui.painter().circle_filled(
                                    logo_rect.center(),
                                    radius,
                                    egui::Color32::from_rgba_unmultiplied(255, 136, 31, alpha),
                                );
                            }
                            Self::paint_logo(ui, logo_rect.shrink(6.0), ACCENT_HI);
                        });
                    });
                } else {
                    ui.vertical(|ui| render_copy(ui));
                }
            });
        Self::glow_outline(ui, shown.response.rect, ACCENT, true);
    }

    fn stat_card(ui: &mut egui::Ui, title: &str, value: &str, subtitle: &str, color: egui::Color32, ok: bool) {
        let accent = if ok { color } else { GREY };
        let shown = egui::Frame::none()
            .fill(PANEL)
            .stroke(egui::Stroke::new(1.0, accent.gamma_multiply(0.55)))
            .rounding(egui::Rounding::same(12.0))
            .inner_margin(egui::Margin::same(16.0))
            .show(ui, |ui| {
                ui.set_min_height(96.0);
                ui.horizontal(|ui| {
                    Self::dot(ui, accent);
                    ui.label(egui::RichText::new(title.to_uppercase()).size(9.2).strong().color(DIM));
                });
                ui.add_space(10.0);
                ui.label(egui::RichText::new(value).size(21.0).strong().color(FG));
                ui.add_space(2.0);
                ui.label(egui::RichText::new(subtitle).size(9.1).color(FAINT));
            });
        Self::glow_outline(ui, shown.response.rect, accent, ok);
    }

    fn action_button(ui: &mut egui::Ui, title: &str, subtitle: &str) -> bool {
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), 64.0),
            egui::Sense::click(),
        );
        let hovered = response.hovered();
        ui.painter().rect_filled(
            rect,
            egui::Rounding::same(10.0),
            if hovered {
                egui::Color32::from_rgb(16, 31, 50)
            } else {
                PANEL_HI
            },
        );
        ui.painter().rect_stroke(
            rect,
            egui::Rounding::same(10.0),
            egui::Stroke::new(
                1.0,
                if hovered { ACCENT } else { egui::Color32::from_rgb(39, 58, 82) },
            ),
        );
        if hovered {
            Self::glow_outline(ui, rect, ACCENT, false);
        }
        ui.painter().text(
            egui::pos2(rect.left() + 15.0, rect.top() + 18.0),
            egui::Align2::LEFT_CENTER,
            title,
            egui::FontId::proportional(11.4),
            FG,
        );
        ui.painter().text(
            egui::pos2(rect.left() + 15.0, rect.top() + 41.0),
            egui::Align2::LEFT_CENTER,
            subtitle,
            egui::FontId::proportional(9.0),
            FAINT,
        );
        ui.painter().text(
            egui::pos2(rect.right() - 17.0, rect.center().y),
            egui::Align2::CENTER_CENTER,
            "›",
            egui::FontId::proportional(20.0),
            if hovered { ACCENT_HI } else { DIM },
        );
        response.clicked()
    }

    fn render_home(&mut self, ui: &mut egui::Ui) {
        self.hero(ui);
        ui.add_space(12.0);

        let studio = self.shared.studio_running.load(Ordering::Relaxed);
        let mcp = self.shared.mcp_alive.load(Ordering::Relaxed);
        let workspace = self.shared.workspace_ready.load(Ordering::Relaxed);
        let extension = self.shared.extension_recent();
        let tools = self.shared.tools.lock().map(|v| v.len()).unwrap_or(0);
        let root = self.shared.workspace_root.lock().map(|v| v.clone()).unwrap_or_default();
        let wide = ui.available_width() >= 940.0;

        if wide {
            ui.columns(4, |cols| {
                Self::stat_card(&mut cols[0], "Bridge", if extension { "Connected" } else { "Waiting" }, "Browser extension link", GREEN, extension);
                Self::stat_card(&mut cols[1], "Roblox Studio", if studio && mcp { "Connected" } else if studio { "Open" } else { "Offline" }, "Primary Studio MCP", AMBER, studio && mcp);
                Self::stat_card(&mut cols[2], "Tools", &tools.to_string(), "Live tool catalog", BLUE, tools > 0);
                Self::stat_card(&mut cols[3], "Workspace", if workspace { "Ready" } else { "Off" }, "AgentScript files", PURPLE, workspace);
            });
        } else {
            ui.columns(2, |cols| {
                Self::stat_card(&mut cols[0], "Bridge", if extension { "Connected" } else { "Waiting" }, "Browser extension link", GREEN, extension);
                Self::stat_card(&mut cols[1], "Roblox Studio", if studio && mcp { "Connected" } else if studio { "Open" } else { "Offline" }, "Primary Studio MCP", AMBER, studio && mcp);
            });
            ui.add_space(8.0);
            ui.columns(2, |cols| {
                Self::stat_card(&mut cols[0], "Tools", &tools.to_string(), "Live tool catalog", BLUE, tools > 0);
                Self::stat_card(&mut cols[1], "Workspace", if workspace { "Ready" } else { "Off" }, "AgentScript files", PURPLE, workspace);
            });
        }

        ui.add_space(12.0);

        let mut render_quick = |ui: &mut egui::Ui| {
            Self::panel().show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Quick actions").size(13.0).strong().color(FG));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(egui::RichText::new("desktop controls").size(8.3).color(FAINT));
                    });
                });
                ui.add_space(9.0);

                let action_wide = ui.available_width() >= 430.0;
                if action_wide {
                    ui.columns(2, |actions| {
                        if Self::action_button(&mut actions[0], "Browse tools", "See every live PlazCode tool") {
                            self.page = Page::Tools;
                        }
                        if Self::action_button(&mut actions[1], "MCP servers", "Manage optional integrations") {
                            self.page = Page::Mcp;
                        }
                    });
                    ui.add_space(8.0);
                    ui.columns(2, |actions| {
                        if Self::action_button(&mut actions[0], "Open terminal", "Inspect bridge and MCP logs") {
                            self.page = Page::Console;
                        }
                        if Self::action_button(&mut actions[1], "Settings", "Adjust synced AI preferences") {
                            self.page = Page::Settings;
                        }
                    });
                } else {
                    if Self::action_button(ui, "Browse tools", "See every live PlazCode tool") { self.page = Page::Tools; }
                    ui.add_space(7.0);
                    if Self::action_button(ui, "MCP servers", "Manage optional integrations") { self.page = Page::Mcp; }
                    ui.add_space(7.0);
                    if Self::action_button(ui, "Open terminal", "Inspect bridge and MCP logs") { self.page = Page::Console; }
                    ui.add_space(7.0);
                    if Self::action_button(ui, "Settings", "Adjust synced AI preferences") { self.page = Page::Settings; }
                }

                ui.add_space(12.0);
                ui.label(egui::RichText::new("Workspace").size(10.0).strong().color(DIM));
                ui.label(
                    egui::RichText::new(if root.is_empty() { "(not configured)" } else { &root })
                        .monospace()
                        .size(8.8)
                        .color(FAINT)
                );

                let prefs = self.preferences.snapshot();
                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    badge(ui, "Engine", if prefs.engine == "local" { "AgentScript" } else { "Roblox" });
                    badge(ui, "Work", &prefs.work_mode);
                    badge(ui, "Reasoning", reasoning_label(&prefs.thinking_level));
                    badge(ui, "Plan", if prefs.plan_mode { "on" } else { "off" });
                });
            });
        };

        let render_recent = |ui: &mut egui::Ui| {
            Self::panel().show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Recent activity").size(13.0).strong().color(FG));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add(
                            egui::Button::new(egui::RichText::new("Clear").size(9.0).color(DIM))
                                .fill(PANEL_HI)
                                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(42, 58, 78)))
                                .rounding(egui::Rounding::same(7.0))
                        ).clicked() {
                            self.shared.clear_logs();
                        }
                    });
                });
                ui.add_space(8.0);

                let mut recent = self.shared.logs.lock()
                    .map(|logs| logs.iter().rev().take(8).cloned().collect::<Vec<_>>())
                    .unwrap_or_default();
                recent.reverse();

                if recent.is_empty() {
                    ui.label(egui::RichText::new("No recent activity yet.").size(9.5).color(FAINT));
                } else {
                    for line in recent {
                        let level = line_level(&line);
                        ui.horizontal(|ui| {
                            Self::dot(ui, level_color(level));
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(line)
                                        .monospace()
                                        .size(8.5)
                                        .color(if level == Level::Info { DIM } else { level_color(level) })
                                ).truncate()
                            );
                        });
                        ui.add_space(3.0);
                    }
                }
            });
        };

        if ui.available_width() >= 860.0 {
            ui.columns(2, |cols| {
                render_quick(&mut cols[0]);
                render_recent(&mut cols[1]);
            });
        } else {
            render_quick(ui);
            ui.add_space(10.0);
            render_recent(ui);
        }
    }

    fn render_tools(&mut self, ui: &mut egui::Ui) {
        Self::section_title(ui, "Tools", "Live tools from Roblox Studio, AgentScript, PlazCode, and enabled MCP servers.");

        let all_tools = self.shared.tools.lock().map(|v| v.clone()).unwrap_or_default();
        let counts = [
            all_tools.len(),
            all_tools.iter().filter(|tool| tool.source == "Roblox Studio").count(),
            all_tools.iter().filter(|tool| tool.source == "AgentScript").count(),
            all_tools.iter().filter(|tool| tool.source.starts_with("PlazCode")).count(),
            all_tools.iter().filter(|tool| tool.source.starts_with("MCP / ")).count(),
        ];

        let search_shell = Self::panel().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.tool_filter)
                        .hint_text("Search tools or sources...")
                        .desired_width(360.0)
                );
                ui.add_space(10.0);

                for (idx, (label, count)) in [
                    ("All", counts[0]),
                    ("RS", counts[1]),
                    ("AS", counts[2]),
                    ("PC", counts[3]),
                    ("MCP", counts[4]),
                ].iter().enumerate() {
                    let selected = self.tool_source == idx;
                    if ui.add(
                        egui::Button::new(
                            egui::RichText::new(format!("{label}  {count}"))
                                .size(9.5)
                                .strong()
                                .color(if selected { INK } else { DIM })
                        )
                        .fill(if selected { ACCENT } else { PANEL_HI })
                        .stroke(egui::Stroke::new(
                            1.0,
                            if selected { ACCENT_HI } else { egui::Color32::from_rgb(39, 58, 82) }
                        ))
                        .rounding(egui::Rounding::same(9.0))
                    ).clicked() {
                        self.tool_source = idx;
                    }
                }
            });
        });
        Self::glow_outline(ui, search_shell.response.rect, ACCENT, false);

        ui.add_space(10.0);
        let needle = self.tool_filter.trim().to_ascii_lowercase();
        let filtered: Vec<ToolDisplay> = all_tools.into_iter()
            .filter(|tool| {
                let source_match = match self.tool_source {
                    1 => tool.source == "Roblox Studio",
                    2 => tool.source == "AgentScript",
                    3 => tool.source.starts_with("PlazCode"),
                    4 => tool.source.starts_with("MCP / "),
                    _ => true,
                };
                let search_match = needle.is_empty()
                    || tool.name.to_ascii_lowercase().contains(&needle)
                    || tool.source.to_ascii_lowercase().contains(&needle);
                source_match && search_match
            })
            .collect();

        egui::ScrollArea::vertical()
            .id_salt("tools-list")
            .max_height(560.0)
            .show(ui, |ui| {
                if filtered.is_empty() {
                    Self::panel().show(ui, |ui| {
                        ui.label(egui::RichText::new("No matching tools are currently available.").size(10.0).color(FAINT));
                    });
                    return;
                }

                let column_count = if ui.available_width() >= 980.0 { 3 } else if ui.available_width() >= 640.0 { 2 } else { 1 };
                ui.columns(column_count, |cols| {
                    for (index, tool) in filtered.iter().enumerate() {
                        let col = &mut cols[index % column_count];
                        let (tag, color) = tool_source_style(&tool.source);
                        let tool_card = Self::glow_panel(if tool.available { color } else { GREY }).show(col, |ui| {
                            ui.set_min_height(74.0);
                            ui.horizontal(|ui| {
                                egui::Frame::none()
                                    .fill(egui::Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 24))
                                    .stroke(egui::Stroke::new(1.0, color.gamma_multiply(0.72)))
                                    .rounding(egui::Rounding::same(6.0))
                                    .inner_margin(egui::Margin::symmetric(7.0, 3.0))
                                    .show(ui, |ui| {
                                        ui.label(egui::RichText::new(tag).size(8.3).strong().color(color));
                                    });
                                ui.label(
                                    egui::RichText::new(&tool.name)
                                        .monospace()
                                        .size(10.8)
                                        .strong()
                                        .color(if tool.available { FG } else { GREY })
                                );
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    Self::dot(ui, if tool.available { GREEN } else { GREY });
                                });
                            });
                            ui.add_space(7.0);
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new(&tool.source).size(8.7).color(FAINT));
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.label(
                                        egui::RichText::new(if tool.available { "AVAILABLE" } else { "OFFLINE" })
                                            .size(7.6)
                                            .strong()
                                            .color(if tool.available { GREEN } else { GREY })
                                    );
                                });
                            });
                        });
                        Self::glow_outline(col, tool_card.response.rect, color, false);
                        col.add_space(9.0);
                    }
                });
            });
    }

    fn render_mcp(&mut self, ui: &mut egui::Ui) {
        Self::section_title(ui, "MCP Servers", "Manage optional MCP integrations. Roblox Studio remains the protected primary server.");

        if let Some((message, ok)) = &self.catalog_notice {
            let color = if *ok { GREEN } else { RED };
            egui::Frame::none()
                .fill(egui::Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 18))
                .stroke(egui::Stroke::new(1.0, color.gamma_multiply(0.8)))
                .rounding(egui::Rounding::same(8.0))
                .inner_margin(egui::Margin::same(9.0))
                .show(ui, |ui| {
                    ui.label(egui::RichText::new(message).size(9.5).color(color));
                });
            ui.add_space(10.0);
        }

        let primary_alive = self.shared.mcp_alive.load(Ordering::Relaxed);
        Self::glow_panel(if primary_alive { GREEN } else { AMBER }).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new("Roblox Studio MCP").size(13.5).strong().color(FG));
                    ui.label(egui::RichText::new("Primary server • built into PlazCode • cannot be removed").size(9.2).color(FAINT));
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    status_pill(ui, if primary_alive { "Active" } else { "Waiting" }, if primary_alive { GREEN } else { AMBER });
                });
            });
        });

        ui.add_space(11.0);
        let live = self.shared.servers.lock().map(|v| v.clone()).unwrap_or_default();
        let entries = mcp_addons::catalog();

        egui::ScrollArea::vertical()
            .id_salt("mcp-list")
            .max_height(545.0)
            .show(ui, |ui| {
                let column_count = if ui.available_width() >= 980.0 { 3 } else if ui.available_width() >= 640.0 { 2 } else { 1 };
                ui.columns(column_count, |cols| {
                    for (index, entry) in entries.iter().enumerate() {
                        let col = &mut cols[index % column_count];
                        let enabled = mcp_addons::is_enabled(entry.id);
                        let status = live.iter().find(|server| server.id == entry.id);
                        let alive = status.map(|server| server.alive).unwrap_or(false);
                        let accent = if alive { GREEN } else if enabled { AMBER } else { egui::Color32::from_rgb(78, 111, 153) };

                        let server_card = Self::glow_panel(accent).show(col, |ui| {
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.label(egui::RichText::new(entry.name).size(12.5).strong().color(FG));
                                    ui.label(egui::RichText::new(entry.description).size(9.1).color(DIM));
                                });
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    let label = if alive { "Active" } else if enabled { "Configured" } else { "Available" };
                                    status_pill(ui, label, accent);
                                });
                            });

                            ui.add_space(9.0);
                            ui.label(
                                egui::RichText::new(format!("{} {}", entry.command, entry.args.join(" ")))
                                    .monospace()
                                    .size(8.4)
                                    .color(FAINT)
                            );

                            if let Some(server) = status {
                                if let Some(error) = &server.error {
                                    ui.add_space(5.0);
                                    ui.label(egui::RichText::new(error).size(8.3).color(RED));
                                } else if server.alive {
                                    ui.add_space(5.0);
                                    ui.label(egui::RichText::new(format!("{} tools available", server.tools)).size(8.5).color(GREEN));
                                }
                            }

                            ui.add_space(9.0);
                            let label = if enabled { "Disable" } else { "Enable" };
                            let color = if enabled { RED } else { ACCENT };
                            if ui.add(
                                egui::Button::new(egui::RichText::new(label).size(9.5).strong().color(if enabled { RED } else { INK }))
                                    .fill(if enabled { PANEL_HI } else { ACCENT })
                                    .stroke(egui::Stroke::new(1.0, color.gamma_multiply(0.8)))
                                    .rounding(egui::Rounding::same(7.0))
                            ).clicked() {
                                match mcp_addons::set_catalog_enabled(entry.id, !enabled) {
                                    Ok(()) => {
                                        self.catalog_notice = Some((
                                            format!("{} {}. Tool discovery refreshes on the next bridge tool-list request.", entry.name, if enabled { "disabled" } else { "enabled" }),
                                            true,
                                        ));
                                    }
                                    Err(error) => self.catalog_notice = Some((error.to_string(), false)),
                                }
                            }
                        });
                        Self::glow_outline(col, server_card.response.rect, accent, alive);
                        col.add_space(9.0);
                    }
                });
            });
    }

    fn render_console(&mut self, ui: &mut egui::Ui) {
        Self::section_title(ui, "Terminal", "Live PlazCode bridge, Studio, MCP, and local agent logs.");

        ui.horizontal(|ui| {
            for (idx, label) in ["All", "Info", "Warn", "Error"].iter().enumerate() {
                let selected = self.log_filter == idx;
                if ui.add(
                    egui::Button::new(egui::RichText::new(*label).size(9.5).strong().color(if selected { INK } else { DIM }))
                        .fill(if selected { ACCENT } else { PANEL_HI })
                        .stroke(egui::Stroke::new(1.0, if selected { ACCENT_HI } else { egui::Color32::from_rgb(42, 58, 78) }))
                        .rounding(egui::Rounding::same(8.0))
                ).clicked() {
                    self.log_filter = idx;
                }
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(
                    egui::Button::new(egui::RichText::new("Clear").size(9.5).color(DIM))
                        .fill(PANEL_HI)
                        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(42, 58, 78)))
                        .rounding(egui::Rounding::same(7.0))
                ).clicked() {
                    self.shared.clear_logs();
                }
                badge(ui, "Tools", &self.shared.tools.lock().map(|v| v.len()).unwrap_or(0).to_string());
                badge(ui, "Studio", if self.shared.mcp_alive.load(Ordering::Relaxed) { "online" } else { "offline" });
                badge(ui, "Bridge", if self.shared.extension_recent() { "live" } else { "waiting" });
            });
        });

        ui.add_space(9.0);
        let logs = self.shared.logs.lock()
            .map(|v| v.iter().cloned().collect::<Vec<_>>())
            .unwrap_or_default();

        egui::Frame::none()
            .fill(TERM)
            .stroke(egui::Stroke::new(1.0, ACCENT.gamma_multiply(0.35)))
            .rounding(egui::Rounding::same(11.0))
            .inner_margin(egui::Margin::same(12.0))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("console-scroll")
                    .stick_to_bottom(true)
                    .max_height(580.0)
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
                                ui.label(
                                    egui::RichText::new(line)
                                        .monospace()
                                        .size(9.4)
                                        .color(level_color(level))
                                );
                            }
                        }
                    });
            });
    }

    fn render_settings(&mut self, ui: &mut egui::Ui) {
        Self::section_title(ui, "Settings", "Desktop controls sync with the extension and autosave immediately.");

        let mut prefs = self.preferences.snapshot();
        let mut changed = false;

        ui.columns(2, |cols| {
            let (left, right) = cols.split_at_mut(1);
            let left = &mut left[0];
            let right = &mut right[0];

            Self::panel().show(left, |ui| {
                ui.label(egui::RichText::new("Execution").size(12.5).strong().color(FG));
                ui.label(egui::RichText::new("Choose where commands run and how much access AgentScript receives.").size(9.0).color(FAINT));
                ui.add_space(9.0);

                ui.label(egui::RichText::new("Engine").size(9.0).strong().color(DIM));
                ui.horizontal(|ui| {
                    changed |= choice(ui, &mut prefs.engine, "roblox", "Roblox Studio");
                    changed |= choice(ui, &mut prefs.engine, "local", "AgentScript");
                });

                ui.add_space(10.0);
                ui.label(egui::RichText::new("AgentScript permissions").size(9.0).strong().color(DIM));
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

            left.add_space(9.0);
            Self::panel().show(left, |ui| {
                ui.label(egui::RichText::new("Reasoning").size(12.5).strong().color(FG));
                ui.label(egui::RichText::new("Matches the extension's work-mode and reasoning controls.").size(9.0).color(FAINT));
                ui.add_space(9.0);

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

                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Reasoning level").size(9.5).strong().color(DIM));
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

                ui.add_space(6.0);
                changed |= toggle_row(ui, "Extra Thinking", "Allow longer reasoning where supported.", &mut prefs.extra_thinking);
                ui.separator();
                changed |= toggle_row(ui, "Plan Mode", "Plan before making project changes.", &mut prefs.plan_mode);
            });

            Self::panel().show(right, |ui| {
                ui.label(egui::RichText::new("Automation").size(12.5).strong().color(FG));
                ui.label(egui::RichText::new("Keep PlazCode helpers enabled without changing provider integrations.").size(9.0).color(FAINT));
                ui.add_space(8.0);
                changed |= toggle_row(ui, "Forge UI", "UI-building helper behavior.", &mut prefs.forge_mode);
                ui.separator();
                changed |= toggle_row(ui, "Auto Fix", "React to supported playtest errors.", &mut prefs.auto_fix);
                ui.separator();
                changed |= toggle_row(ui, "Background Mode", "Continue the browser agent loop while unfocused.", &mut prefs.bg_mode);
                ui.separator();
                changed |= toggle_row(ui, "Sounds", "Play PlazCode UI feedback sounds.", &mut prefs.sounds);
            });

            right.add_space(9.0);
            Self::glow_panel(GREEN).show(right, |ui| {
                ui.horizontal(|ui| {
                    Self::dot(ui, GREEN);
                    ui.label(egui::RichText::new("Autosave active").size(11.0).strong().color(FG));
                });
                ui.add_space(5.0);
                ui.label(egui::RichText::new("Every change is written immediately and restored after reopening PlazCode.").size(9.0).color(DIM));
                ui.label(egui::RichText::new(format!("{}", self.preferences.path().display())).monospace().size(8.4).color(FAINT));
            });

            right.add_space(9.0);
            Self::panel().show(right, |ui| {
                ui.label(egui::RichText::new("Desktop app").size(12.5).strong().color(FG));
                ui.label(egui::RichText::new("Closing the window keeps the bridge running. Launch PlazCode.exe again to reopen it.").size(9.0).color(FAINT));
                ui.add_space(9.0);
                ui.horizontal(|ui| {
                    if ui.add(
                        egui::Button::new(egui::RichText::new("Hide to background").size(9.5).strong().color(INK))
                            .fill(ACCENT)
                            .stroke(egui::Stroke::new(1.0, ACCENT_HI))
                            .rounding(egui::Rounding::same(7.0))
                    ).clicked() {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Visible(false));
                        self.shared.log("desktop window hidden — PlazCode is still running in the background");
                    }

                    if ui.add(
                        egui::Button::new(egui::RichText::new("Exit PlazCode").size(9.5).strong().color(RED))
                            .fill(PANEL_HI)
                            .stroke(egui::Stroke::new(1.0, RED.gamma_multiply(0.6)))
                            .rounding(egui::Rounding::same(7.0))
                    ).clicked() {
                        self.shared.request_quit();
                    }
                });
            });
        });

        if changed {
            let full = prefs.perm_mode == "full";
            match self.preferences.replace(prefs) {
                Ok(_) => {
                    if let Ok(flag) = self.shared.full_access.read() {
                        flag.store(full, Ordering::Relaxed);
                    }
                }
                Err(error) => self.shared.log(&format!("ERROR desktop settings save failed: {error:#}")),
            }
        }
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
                Self::paint_background(ui);
                self.render_header(ui);
                egui::ScrollArea::vertical()
                    .id_salt("plazcode-page-scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        match self.page {
                            Page::Home => self.render_home(ui),
                            Page::Tools => self.render_tools(ui),
                            Page::Mcp => self.render_mcp(ui),
                            Page::Console => self.render_console(ui),
                            Page::Settings => self.render_settings(ui),
                        }
                        ui.add_space(8.0);
                    });
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

fn tool_source_style(source: &str) -> (&'static str, egui::Color32) {
    if source == "Roblox Studio" {
        ("RS", egui::Color32::from_rgb(74, 157, 255))
    } else if source == "AgentScript" {
        ("AS", egui::Color32::from_rgb(161, 110, 255))
    } else if source.starts_with("MCP / ") {
        ("MCP", GREEN)
    } else {
        ("PC", ACCENT)
    }
}

fn status_pill(ui: &mut egui::Ui, label: &str, color: egui::Color32) {
    egui::Frame::none()
        .fill(egui::Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 24))
        .stroke(egui::Stroke::new(1.0, color.gamma_multiply(0.65)))
        .rounding(egui::Rounding::same(8.0))
        .inner_margin(egui::Margin::symmetric(8.0, 4.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                AgentApp::dot(ui, color);
                ui.label(egui::RichText::new(label).size(8.8).strong().color(color));
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

#[cfg(test)]
mod tests {
    use super::*;

    fn names(rows: &[ToolDisplay]) -> Vec<(String, String)> {
        rows.iter().map(|row| (row.name.clone(), row.source.clone())).collect()
    }

    #[test]
    fn tool_catalog_merges_all_sources() {
        let shared = UiShared::new(Arc::new(AtomicBool::new(false)));

        shared.set_roblox_tools(
            &[serde_json::json!({"name":"execute_luau"})],
            true,
        );
        shared.set_local_tools(
            &[serde_json::json!({"name":"read_file"})],
            true,
        );
        shared.set_addon_tools(
            &[serde_json::json!({"name":"memory__search","server":"memory"})],
            Vec::new(),
        );
        shared.set_browser_tools(
            &[
                serde_json::json!({"name":"web_search","source":"PlazCode"}),
                serde_json::json!({"name":"animation_create","source":"PlazCode / Motion"}),
            ],
        );

        let rows = shared.tools.lock().expect("tool lock").clone();
        let names = names(&rows);
        assert!(names.contains(&("execute_luau".to_string(), "Roblox Studio".to_string())));
        assert!(names.contains(&("read_file".to_string(), "AgentScript".to_string())));
        assert!(names.contains(&("memory__search".to_string(), "MCP / memory".to_string())));
        assert!(names.contains(&("web_search".to_string(), "PlazCode".to_string())));
        assert!(names.contains(&("animation_create".to_string(), "PlazCode / Motion".to_string())));
    }

    #[test]
    fn browser_tool_refresh_replaces_only_browser_rows() {
        let shared = UiShared::new(Arc::new(AtomicBool::new(false)));
        shared.set_local_tools(&[serde_json::json!({"name":"tree"})], true);
        shared.set_browser_tools(&[serde_json::json!({"name":"web_search","source":"PlazCode"})]);
        shared.set_browser_tools(&[serde_json::json!({"name":"web_fetch","source":"PlazCode"})]);

        let rows = shared.tools.lock().expect("tool lock").clone();
        assert!(rows.iter().any(|row| row.name == "tree" && row.source == "AgentScript"));
        assert!(rows.iter().any(|row| row.name == "web_fetch" && row.source == "PlazCode"));
        assert!(!rows.iter().any(|row| row.name == "web_search" && row.source == "PlazCode"));
    }
}

fn window_icon() -> Arc<egui::IconData> {
    let width = 64usize;
    let height = 64usize;
    let mut rgba = vec![0u8; width * height * 4];

    let set = |rgba: &mut [u8], x: usize, y: usize, color: [u8; 4]| {
        if x >= width || y >= height {
            return;
        }
        let i = (y * width + x) * 4;
        rgba[i..i + 4].copy_from_slice(&color);
    };

    let inside_round = |x: i32, y: i32| -> bool {
        let left = 3;
        let top = 3;
        let right = 60;
        let bottom = 60;
        let radius = 11;

        if x >= left + radius && x <= right - radius && y >= top && y <= bottom {
            return true;
        }
        if y >= top + radius && y <= bottom - radius && x >= left && x <= right {
            return true;
        }

        let corners = [
            (left + radius, top + radius),
            (right - radius, top + radius),
            (left + radius, bottom - radius),
            (right - radius, bottom - radius),
        ];
        corners.iter().any(|(cx, cy)| {
            let dx = x - cx;
            let dy = y - cy;
            dx * dx + dy * dy <= radius * radius
        })
    };

    for y in 0..height {
        for x in 0..width {
            if inside_round(x as i32, y as i32) {
                set(&mut rgba, x, y, [6, 20, 38, 255]);
            }
        }
    }

    let orange = [255, 164, 48, 255];
    let glow = [255, 192, 87, 255];

    for y in 14..40 {
        for x in 17..23 {
            set(&mut rgba, x, y, orange);
        }
    }
    for y in 14..20 {
        for x in 22..42 {
            set(&mut rgba, x, y, glow);
        }
    }
    for y in 29..35 {
        for x in 22..39 {
            set(&mut rgba, x, y, orange);
        }
    }

    for y in 16..34 {
        let dy = y as i32 - 25;
        for x in 34..48 {
            let dx = x as i32 - 36;
            let outer = dx * dx + dy * dy <= 12 * 12;
            let inner = dx * dx + dy * dy <= 6 * 6;
            if outer && !inner {
                set(&mut rgba, x, y, orange);
            }
        }
    }

    for y in 43..55 {
        let offset = (y - 43) / 2;
        for x in (18usize.saturating_sub(offset))..(24usize.saturating_sub(offset)) {
            set(&mut rgba, x, y, orange);
        }
        for x in (30usize.saturating_sub(offset))..(36usize.saturating_sub(offset)) {
            set(&mut rgba, x, y, orange);
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
            .with_inner_size([1240.0, 780.0])
            .with_min_inner_size([820.0, 580.0])
            .with_icon(window_icon())
            .with_title("PlazCode"),
        ..Default::default()
    };

    eframe::run_native(
        "PlazCode",
        options,
        Box::new(move |cc| {
            install_style(&cc.egui_ctx);
            Ok(Box::new(AgentApp {
                shared,
                restart_tx,
                preferences,
                booted: Instant::now(),
                page: Page::Home,
                log_filter: 0,
                tool_filter: String::new(),
                tool_source: 0,
                catalog_notice: None,
            }))
        }),
    )
}

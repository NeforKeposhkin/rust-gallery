use chrono::{DateTime, Local};
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::collections::{hash_map::DefaultHasher, HashSet};
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;

const ROOT_DIR: &str = "/storage/emulated/0";
const TRASH_DIR: &str = "/storage/emulated/0/.RGE_Trash";
const THUMB_CACHE_DIR: &str = "/storage/emulated/0/.RGE_Thumbs";
const CONFIG_PATH: &str = "/storage/emulated/0/.rge_config.json";

#[derive(Serialize, Deserialize, Clone, PartialEq)]
enum DeletePolicy {
    AskEveryTime,
    MoveToTrash,
    DeletePermanently,
}

#[derive(Serialize, Deserialize, Clone, PartialEq)]
enum AnimStyle {
    None,
    ExpressiveSpring,
    SmoothFade,
}

#[derive(Serialize, Deserialize, Clone, PartialEq)]
enum ViewMode {
    List,
    Grid,
}

#[derive(Serialize, Deserialize, Clone, PartialEq)]
enum M3ThemeSeed {
    LavenderPurple, // Фирменный Android 17 Expressive
    EmeraldMint,
    SunsetCoral,
    OceanBlue,
    CustomColor,
}

#[derive(Clone, PartialEq)]
enum ClipboardOp {
    Copy,
    Move,
}

#[derive(Serialize, Deserialize, Clone)]
struct AppSettings {
    ui_scale: f32,
    font_size: f32,
    top_padding: f32,
    theme_seed: M3ThemeSeed,
    custom_bg_color: [u8; 3],
    bg_image_path: Option<String>,
    bg_dimming: f32,
    anim_style: AnimStyle,
    anim_speed: f32,
    delete_policy: DeletePolicy,
    explorer_view: ViewMode,
    grid_columns: usize,
    favorites: HashSet<PathBuf>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            ui_scale: 1.25,
            font_size: 16.0,
            top_padding: 40.0,
            theme_seed: M3ThemeSeed::LavenderPurple,
            custom_bg_color: [20, 18, 24],
            bg_image_path: None,
            bg_dimming: 0.65,
            anim_style: AnimStyle::ExpressiveSpring,
            anim_speed: 0.28,
            delete_policy: DeletePolicy::AskEveryTime,
            explorer_view: ViewMode::List,
            grid_columns: 3,
            favorites: HashSet::new(),
        }
    }
}

impl AppSettings {
    fn load() -> Self {
        if let Ok(data) = fs::read_to_string(CONFIG_PATH) {
            if let Ok(cfg) = serde_json::from_str(&data) {
                return cfg;
            }
        }
        Self::default()
    }

    fn save(&self) {
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = fs::write(CONFIG_PATH, json);
        }
    }

    // Генерация тональной палитры Material 3 Expressive
    fn m3_palette(&self) -> (egui::Color32, egui::Color32, egui::Color32, egui::Color32) {
        // (Surface, SurfaceContainer, PrimaryContainer, OnPrimaryContainer)
        match self.theme_seed {
            M3ThemeSeed::LavenderPurple => (
                egui::Color32::from_rgb(20, 18, 24),
                egui::Color32::from_rgb(33, 31, 38),
                egui::Color32::from_rgb(79, 55, 139),
                egui::Color32::from_rgb(234, 221, 255),
            ),
            M3ThemeSeed::EmeraldMint => (
                egui::Color32::from_rgb(15, 21, 18),
                egui::Color32::from_rgb(26, 35, 31),
                egui::Color32::from_rgb(24, 82, 62),
                egui::Color32::from_rgb(188, 242, 218),
            ),
            M3ThemeSeed::SunsetCoral => (
                egui::Color32::from_rgb(25, 17, 16),
                egui::Color32::from_rgb(39, 29, 27),
                egui::Color32::from_rgb(122, 47, 36),
                egui::Color32::from_rgb(255, 218, 212),
            ),
            M3ThemeSeed::OceanBlue => (
                egui::Color32::from_rgb(16, 20, 25),
                egui::Color32::from_rgb(27, 33, 41),
                egui::Color32::from_rgb(26, 73, 117),
                egui::Color32::from_rgb(209, 228, 255),
            ),
            M3ThemeSeed::CustomColor => {
                let c = self.custom_bg_color;
                let surf = egui::Color32::from_rgb(c[0], c[1], c[2]);
                let cont = egui::Color32::from_rgb(
                    c[0].saturating_add(16),
                    c[1].saturating_add(16),
                    c[2].saturating_add(20),
                );
                (
                    surf,
                    cont,
                    egui::Color32::from_rgb(79, 55, 139),
                    egui::Color32::from_rgb(234, 221, 255),
                )
            }
        }
    }
}

#[derive(PartialEq, Clone, Copy)]
enum ActiveTab {
    Explorer,
    Gallery,
    Favorites,
    Trash,
    Settings,
}

struct ImageEditorState {
    path: PathBuf,
    rotation: u8,
    flip_h: bool,
    brightness: i32,
    contrast: f32,
    grayscale: bool,
    crop_margin: f32,
    draw_mode: bool,
    brush_color: egui::Color32,
    brush_size: f32,
    strokes: Vec<(Vec<egui::Pos2>, egui::Color32, f32)>,
}

impl ImageEditorState {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            rotation: 0,
            flip_h: false,
            brightness: 0,
            contrast: 0.0,
            grayscale: false,
            crop_margin: 0.0,
            draw_mode: false,
            brush_color: egui::Color32::from_rgb(255, 95, 86),
            brush_size: 5.0,
            strokes: Vec::new(),
        }
    }

    fn apply_and_save(&self, ctx: &egui::Context) -> Result<PathBuf, String> {
        let mut img = image::open(&self.path).map_err(|e| e.to_string())?;

        if self.crop_margin > 0.01 {
            let w = img.width();
            let h = img.height();
            let dx = (w as f32 * self.crop_margin) as u32;
            let dy = (h as f32 * self.crop_margin) as u32;
            if w > dx * 2 && h > dy * 2 {
                img = img.crop_imm(dx, dy, w - dx * 2, h - dy * 2);
            }
        }

        match self.rotation % 4 {
            1 => img = img.rotate90(),
            2 => img = img.rotate180(),
            3 => img = img.rotate270(),
            _ => {}
        }
        if self.flip_h {
            img = img.fliph();
        }
        if self.brightness != 0 {
            img = img.brighten(self.brightness);
        }
        if self.contrast.abs() > 0.1 {
            img = img.adjust_contrast(self.contrast);
        }
        if self.grayscale {
            img = img.grayscale();
        }

        let parent = self.path.parent().unwrap_or_else(|| Path::new(ROOT_DIR));
        let stem = self.path.file_stem().and_then(|s| s.to_str()).unwrap_or("edited");
        let new_path = parent.join(format!("{}_rge.jpg", stem));

        img.save(&new_path).map_err(|e| e.to_string())?;
        ctx.forget_image(&format!("file://{}", new_path.display()));
        Ok(new_path)
    }
}

struct VideoEditorState {
    path: PathBuf,
    start_sec: f32,
    duration_sec: f32,
    rotation: u16,
    mute_audio: bool,
    speed: f32,
}

impl VideoEditorState {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            start_sec: 0.0,
            duration_sec: 15.0,
            rotation: 0,
            mute_audio: false,
            speed: 1.0,
        }
    }

    fn export_video(&self) -> Result<PathBuf, String> {
        let parent = self.path.parent().unwrap_or_else(|| Path::new(ROOT_DIR));
        let stem = self.path.file_stem().and_then(|s| s.to_str()).unwrap_or("video");
        let out_path = parent.join(format!("{}_rge_edit.mp4", stem));

        let mut args = vec![
            "-y".to_string(),
            "-ss".to_string(),
            format!("{:.1}", self.start_sec),
            "-i".to_string(),
            self.path.to_string_lossy().to_string(),
            "-t".to_string(),
            format!("{:.1}", self.duration_sec),
        ];

        if self.mute_audio {
            args.push("-an".to_string());
        }
        if self.rotation != 0 {
            args.push("-metadata:s:v:0".to_string());
            args.push(format!("rotate={}", self.rotation));
        }
        args.push("-c".to_string());
        args.push("copy".to_string());
        args.push(out_path.to_string_lossy().to_string());

        match Command::new("ffmpeg").args(&args).output() {
            Ok(out) if out.status.success() => Ok(out_path),
            _ => {
                // Если в прошивке нет CLI ffmpeg, передаем файл в системный видеоредактор Android
                let uri = format!("file://{}", self.path.display());
                let _ = Command::new("am")
                    .args([
                        "start",
                        "-a",
                        "android.intent.action.EDIT",
                        "-d",
                        &uri,
                        "-t",
                        "video/*",
                        "--grant-read-uri-permission",
                    ])
                    .spawn();
                Err("Открыт системный видеоредактор Android для рендера".into())
            }
        }
    }
}

struct RgeApp {
    settings: AppSettings,
    active_tab: ActiveTab,
    current_dir: PathBuf,
    entries: Vec<PathBuf>,
    all_media: Vec<PathBuf>,
    selected_items: HashSet<PathBuf>,
    multi_select_mode: bool,
    clipboard: Option<(ClipboardOp, Vec<PathBuf>)>,
    action_target: Option<PathBuf>,
    info_targets: Option<Vec<PathBuf>>,
    viewing_media: Option<PathBuf>,
    img_editor: Option<ImageEditorState>,
    vid_editor: Option<VideoEditorState>,
    pending_delete: Vec<PathBuf>,
    show_delete_modal: bool,
    status_toast: Option<String>,
    anim_progress: f32,
    // Кэш быстрых миниатюр (анти-лаг)
    thumb_in_progress: Arc<Mutex<HashSet<PathBuf>>>,
}

impl Default for RgeApp {
    fn default() -> Self {
        let _ = fs::create_dir_all(TRASH_DIR);
        let _ = fs::create_dir_all(THUMB_CACHE_DIR);
        let root = PathBuf::from(ROOT_DIR);
        let current_dir = if root.exists() { root } else { PathBuf::from("/") };

        let mut app = Self {
            settings: AppSettings::load(),
            active_tab: ActiveTab::Explorer,
            current_dir,
            entries: Vec::new(),
            all_media: Vec::new(),
            selected_items: HashSet::new(),
            multi_select_mode: false,
            clipboard: None,
            action_target: None,
            info_targets: None,
            viewing_media: None,
            img_editor: None,
            vid_editor: None,
            pending_delete: Vec::new(),
            show_delete_modal: false,
            status_toast: None,
            anim_progress: 1.0,
            thumb_in_progress: Arc::new(Mutex::new(HashSet::new())),
        };
        app.refresh_dir();
        app
    }
}

impl RgeApp {
    // Применение стиля Material 3 Expressive (Android 17)
    fn apply_m3_expressive_style(&self, ctx: &egui::Context) {
        ctx.set_pixels_per_point(self.settings.ui_scale);
        let (surface, surface_cont, primary_cont, on_primary_cont) = self.settings.m3_palette();

        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = surface;
        visuals.window_fill = surface_cont;
        visuals.window_rounding = egui::Rounding::same(28.0);
        visuals.menu_rounding = egui::Rounding::same(20.0);

        // Expressive Pill-кнопки (скругление 20dp)
        visuals.widgets.inactive.rounding = egui::Rounding::same(20.0);
        visuals.widgets.inactive.bg_fill = surface_cont;
        visuals.widgets.inactive.weak_bg_fill = surface_cont;

        visuals.widgets.hovered.rounding = egui::Rounding::same(20.0);
        visuals.widgets.hovered.bg_fill = primary_cont;

        visuals.widgets.active.rounding = egui::Rounding::same(20.0);
        visuals.widgets.active.bg_fill = primary_cont;

        visuals.selection.bg_fill = primary_cont;
        visuals.selection.stroke = egui::Stroke::new(1.5, on_primary_cont);

        ctx.set_visuals(visuals);

        let mut style = (*ctx.style()).clone();
        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        style.spacing.button_padding = egui::vec2(14.0, 8.0);
        style.text_styles.insert(
            egui::TextStyle::Body,
            egui::FontId::proportional(self.settings.font_size),
        );
        style.text_styles.insert(
            egui::TextStyle::Button,
            egui::FontId::proportional(self.settings.font_size),
        );
        style.text_styles.insert(
            egui::TextStyle::Heading,
            egui::FontId::proportional(self.settings.font_size * 1.25),
        );
        ctx.set_style(style);
    }

    // Оптимизация: получение или фоновая генерация легкой миниатюры (220x220)
    fn get_fast_thumbnail_uri(&self, orig_path: &Path) -> String {
        let mut hasher = DefaultHasher::new();
        orig_path.hash(&mut hasher);
        let hash = hasher.finish();
        let thumb_path = Path::new(THUMB_CACHE_DIR).join(format!("{:x}.jpg", hash));

        if thumb_path.exists() {
            return format!("file://{}", thumb_path.display());
        }

        // Запускаем фоновый поток сжатия, чтобы UI не лагал при прокрутке
        let mut lock = self.thumb_in_progress.lock().unwrap();
        if lock.len() < 4 && !lock.contains(orig_path) {
            lock.insert(orig_path.to_path_buf());
            let src = orig_path.to_path_buf();
            let dst = thumb_path.clone();
            let tracker = Arc::clone(&self.thumb_in_progress);

            thread::spawn(move || {
                if let Ok(img) = image::open(&src) {
                    let thumb = img.thumbnail(220, 220);
                    let _ = thumb.save(&dst);
                }
                if let Ok(mut l) = tracker.lock() {
                    l.remove(&src);
                }
            });
        }

        format!("file://{}", orig_path.display())
    }

    fn trigger_anim(&mut self) {
        self.anim_progress = if self.settings.anim_style != AnimStyle::None { 0.0 } else { 1.0 };
    }

    fn anim_factor(&self) -> f32 {
        let t = self.anim_progress;
        match self.settings.anim_style {
            AnimStyle::None => 1.0,
            AnimStyle::SmoothFade => t * t * (3.0 - 2.0 * t),
            // Пружинный эффект Material 3 Expressive
            AnimStyle::ExpressiveSpring => {
                1.0 - (1.0 - t).powi(3) * (1.0 - 0.15 * (t * std::f32::consts::PI).sin())
            }
        }
    }

    fn refresh_dir(&mut self) {
        self.entries.clear();
        self.selected_items.clear();

        if let Ok(read_dir) = fs::read_dir(&self.current_dir) {
            let mut dirs = Vec::new();
            let mut files = Vec::new();

            for entry in read_dir.flatten() {
                let path = entry.path();
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.starts_with('.') {
                        continue;
                    }
                }
                if path.is_dir() {
                    dirs.push(path);
                } else {
                    files.push(path);
                }
            }
            dirs.sort();
            files.sort();
            self.entries.extend(dirs);
            self.entries.extend(files);
        }
    }

    fn scan_all_media(&mut self) {
        self.all_media.clear();
        let mut stack = vec![PathBuf::from(ROOT_DIR)];
        while let Some(dir) = stack.pop() {
            if let Ok(rd) = fs::read_dir(&dir) {
                for entry in rd.flatten() {
                    let p = entry.path();
                    if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
                        if name.starts_with('.') || name == "Android" {
                            continue;
                        }
                    }
                    if p.is_dir() {
                        stack.push(p);
                    } else if Self::is_image(&p) || Self::is_video(&p) {
                        self.all_media.push(p);
                    }
                }
            }
        }
        self.all_media.sort();
        self.all_media.reverse();
    }

    fn is_image(path: &Path) -> bool {
        path.extension()
            .and_then(|s| s.to_str())
            .map(|e| matches!(e.to_lowercase().as_str(), "jpg" | "jpeg" | "png" | "webp" | "gif" | "bmp"))
            .unwrap_or(false)
    }

    fn is_video(path: &Path) -> bool {
        path.extension()
            .and_then(|s| s.to_str())
            .map(|e| matches!(e.to_lowercase().as_str(), "mp4" | "mkv" | "webm" | "avi" | "mov" | "3gp"))
            .unwrap_or(false)
    }

    fn format_size(bytes: u64) -> String {
        const KB: u64 = 1024;
        const MB: u64 = KB * 1024;
        const GB: u64 = MB * 1024;
        if bytes >= GB {
            format!("{:.2} ГБ ({} Б)", bytes as f64 / GB as f64, bytes)
        } else if bytes >= MB {
            format!("{:.2} МБ ({} Б)", bytes as f64 / MB as f64, bytes)
        } else if bytes >= KB {
            format!("{:.1} КБ ({} Б)", bytes as f64 / KB as f64, bytes)
        } else {
            format!("{} Б", bytes)
        }
    }

    fn copy_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
        if src.is_dir() {
            fs::create_dir_all(dst)?;
            for entry in fs::read_dir(src)? {
                let entry = entry?;
                Self::copy_recursive(&entry.path(), &dst.join(entry.file_name()))?;
            }
        } else {
            fs::copy(src, dst)?;
        }
        Ok(())
    }

    fn apply_clipboard_here(&mut self) {
        if let Some((op, items)) = self.clipboard.take() {
            let mut count = 0;
            for src in items {
                if let Some(name) = src.file_name() {
                    let mut dest = self.current_dir.join(name);
                    if dest == src && op == ClipboardOp::Copy {
                        let stem = src.file_stem().and_then(|s| s.to_str()).unwrap_or("copy");
                        let ext = src.extension().and_then(|s| s.to_str()).unwrap_or("");
                        let new_name = if ext.is_empty() {
                            format!("{}_copy", stem)
                        } else {
                            format!("{}_copy.{}", stem, ext)
                        };
                        dest = self.current_dir.join(new_name);
                    }

                    let res = match op {
                        ClipboardOp::Move => fs::rename(&src, &dest).or_else(|_| {
                            Self::copy_recursive(&src, &dest)?;
                            if src.is_dir() { fs::remove_dir_all(&src) } else { fs::remove_file(&src) }
                        }),
                        ClipboardOp::Copy => Self::copy_recursive(&src, &dest),
                    };
                    if res.is_ok() {
                        count += 1;
                    }
                }
            }
            self.status_toast = Some(format!("Обработано объектов: {}", count));
            self.refresh_dir();
        }
    }

    fn request_delete(&mut self, paths: Vec<PathBuf>) {
        if paths.is_empty() {
            return;
        }
        match self.settings.delete_policy {
            DeletePolicy::MoveToTrash => self.execute_delete(paths, true),
            DeletePolicy::DeletePermanently => self.execute_delete(paths, false),
            DeletePolicy::AskEveryTime => {
                self.pending_delete = paths;
                self.show_delete_modal = true;
            }
        }
    }

    fn execute_delete(&mut self, paths: Vec<PathBuf>, to_trash: bool) {
        let _ = fs::create_dir_all(TRASH_DIR);
        for p in paths {
            if to_trash && !p.starts_with(TRASH_DIR) {
                if let Some(name) = p.file_name() {
                    let dest = Path::new(TRASH_DIR).join(name);
                    let _ = fs::rename(&p, dest);
                }
            } else if p.is_dir() {
                let _ = fs::remove_dir_all(&p);
            } else {
                let _ = fs::remove_file(&p);
            }
        }
        self.selected_items.clear();
        self.multi_select_mode = false;
        self.viewing_media = None;
        self.refresh_dir();
        if self.active_tab == ActiveTab::Gallery {
            self.scan_all_media();
        }
    }

    fn android_intent(action: &str, path: &Path, mime: &str) {
        let uri = format!("file://{}", path.display());
        let _ = Command::new("am")
            .args([
                "start",
                "-a",
                action,
                "-d",
                &uri,
                "-t",
                mime,
                "--grant-read-uri-permission",
            ])
            .spawn();
    }

    fn share_paths(paths: &[PathBuf]) {
        if let Some(first) = paths.first() {
            let mime = if Self::is_video(first) {
                "video/*"
            } else if Self::is_image(first) {
                "image/*"
            } else {
                "*/*"
            };
            Self::android_intent("android.intent.action.SEND", first, mime);
        }
    }
}

impl eframe::App for RgeApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.apply_m3_expressive_style(ctx);

        if self.anim_progress < 1.0 {
            let dt = ctx.input(|i| i.unstable_dt).max(0.016);
            self.anim_progress = (self.anim_progress + dt / self.settings.anim_speed.max(0.05)).min(1.0);
            ctx.request_repaint();
        }

        // МОДАЛЬНОЕ ОКНО: ПОДТВЕРЖДЕНИЕ УДАЛЕНИЯ
        if self.show_delete_modal {
            egui::Window::new("Удаление")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label(format!("Объектов: {}", self.pending_delete.len()));
                    ui.add_space(10.0);
                    if ui.button("🗑 В корзину").clicked() {
                        let list = std::mem::take(&mut self.pending_delete);
                        self.execute_delete(list, true);
                        self.show_delete_modal = false;
                    }
                    if ui.button("🔥 Удалить навсегда").clicked() {
                        let list = std::mem::take(&mut self.pending_delete);
                        self.execute_delete(list, false);
                        self.show_delete_modal = false;
                    }
                    if ui.button("Отмена").clicked() {
                        self.pending_delete.clear();
                        self.show_delete_modal = false;
                    }
                });
        }

        // МОДАЛЬНОЕ ОКНО: МЕНЮ ДЕЙСТВИЙ ДЛЯ ОДИНОЧНОГО ОБЪЕКТА
        if let Some(target) = self.action_target.clone() {
            egui::Window::new("RGE • Действия")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    let name = target.file_name().unwrap_or_default().to_string_lossy();
                    ui.label(format!("Объект: {}", name));
                    ui.separator();

                    if ui.button("📋 Копировать").clicked() {
                        self.clipboard = Some((ClipboardOp::Copy, vec![target.clone()]));
                        self.status_toast = Some("Скопировано. Перейдите в папку и нажмите «Вставить».".into());
                        self.action_target = None;
                    }
                    if ui.button("✂ Переместить").clicked() {
                        self.clipboard = Some((ClipboardOp::Move, vec![target.clone()]));
                        self.status_toast = Some("Выбрано для переноса. Откройте папку и нажмите «Вставить».".into());
                        self.action_target = None;
                    }
                    if !target.is_dir() && ui.button("📤 Отправить").clicked() {
                        Self::share_paths(&[target.clone()]);
                        self.action_target = None;
                    }
                    if ui.button("ℹ Сведения").clicked() {
                        self.info_targets = Some(vec![target.clone()]);
                        self.action_target = None;
                    }
                    if ui.button("🗑 Удалить").clicked() {
                        self.request_delete(vec![target]);
                        self.action_target = None;
                    }
                    ui.separator();
                    if ui.button("Закрыть").clicked() {
                        self.action_target = None;
                    }
                });
        }

        // МОДАЛЬНОЕ ОКНО: СВЕДЕНИЯ
        if let Some(targets) = self.info_targets.clone() {
            egui::Window::new("ℹ Сведения")
                .collapsible(false)
                .resizable(true)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    egui::ScrollArea::vertical().max_height(350.0).show(ui, |ui| {
                        if targets.len() == 1 {
                            let p = &targets[0];
                            ui.label(format!("Название: {}", p.file_name().unwrap_or_default().to_string_lossy()));
                            ui.label(format!("Путь: {}", p.display()));

                            if let Ok(meta) = fs::metadata(p) {
                                let size_str = if meta.is_dir() {
                                    "Папка".to_string()
                                } else {
                                    Self::format_size(meta.len())
                                };
                                ui.label(format!("Вес: {}", size_str));

                                if let Ok(created) = meta.created() {
                                    let dt: DateTime<Local> = created.into();
                                    ui.label(format!("Создан: {}", dt.format("%d.%m.%Y %H:%M:%S")));
                                } else {
                                    ui.label("Создан: недоступно на ФС");
                                }

                                if let Ok(modified) = meta.modified() {
                                    let dt: DateTime<Local> = modified.into();
                                    ui.label(format!("Изменен: {}", dt.format("%d.%m.%Y %H:%M:%S")));
                                }
                            }

                            ui.separator();
                            ui.label("Доступные функции:");
                            if Self::is_image(p) {
                                ui.label("• Просмотр, Фоторедактор, Обои RGE, Избранное, Отправка, Копирование, Перемещение, Удаление");
                            } else if Self::is_video(p) {
                                ui.label("• Просмотр, Видеоредактор, Избранное, Отправка, Копирование, Перемещение, Удаление");
                            } else if p.is_dir() {
                                ui.label("• Навигация, Копирование, Перемещение, Удаление");
                            } else {
                                ui.label("• Отправка, Копирование, Перемещение, Удаление");
                            }
                        } else {
                            let mut total_bytes = 0u64;
                            for p in &targets {
                                if let Ok(m) = fs::metadata(p) {
                                    if m.is_file() {
                                        total_bytes += m.len();
                                    }
                                }
                            }
                            ui.label(format!("Выбрано объектов: {}", targets.len()));
                            ui.label(format!("Общий вес: {}", Self::format_size(total_bytes)));
                            ui.label("Функции: Групповое копирование, Перемещение, Отправка, Избранное, Удаление");
                        }
                    });
                    ui.add_space(10.0);
                    if ui.button("Закрыть").clicked() {
                        self.info_targets = None;
                    }
                });
        }

        // ВЕРХНЯЯ ПАНЕЛЬ MATERIAL 3 EXPRESSIVE
        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.add_space(self.settings.top_padding);

            ui.horizontal_wrapped(|ui| {
                ui.heading("RGE");
                ui.separator();

                let can_go_up = self.viewing_media.is_some()
                    || self.img_editor.is_some()
                    || self.vid_editor.is_some()
                    || (self.active_tab == ActiveTab::Explorer && self.current_dir != Path::new(ROOT_DIR));

                if ui.add_enabled(can_go_up, egui::Button::new("⬅ Назад")).clicked() {
                    if self.img_editor.is_some() {
                        self.img_editor = None;
                    } else if self.vid_editor.is_some() {
                        self.vid_editor = None;
                    } else if self.viewing_media.is_some() {
                        self.viewing_media = None;
                    } else if self.current_dir != Path::new(ROOT_DIR) {
                        if let Some(parent) = self.current_dir.parent() {
                            if parent.starts_with(ROOT_DIR) {
                                self.current_dir = parent.to_path_buf();
                                self.refresh_dir();
                                self.trigger_anim();
                            }
                        }
                    }
                }

                // Буфер обмена
                if self.active_tab == ActiveTab::Explorer
                    && self.viewing_media.is_none()
                    && self.img_editor.is_none()
                    && self.vid_editor.is_none()
                {
                    if let Some((op, ref items)) = self.clipboard {
                        let lbl = match op {
                            ClipboardOp::Copy => format!("📥 Вставить ({})", items.len()),
                            ClipboardOp::Move => format!("📥 Переместить ({})", items.len()),
                        };
                        if ui.button(lbl).clicked() {
                            self.apply_clipboard_here();
                        }
                        if ui.button("✖").clicked() {
                            self.clipboard = None;
                        }
                    }
                }

                // Множественный выбор
                if self.viewing_media.is_none()
                    && self.img_editor.is_none()
                    && self.vid_editor.is_none()
                    && self.active_tab != ActiveTab::Settings
                {
                    let sel_label = if self.multi_select_mode {
                        format!("☑ Выбрано: {}", self.selected_items.len())
                    } else {
                        "☐ Выбрать".to_string()
                    };
                    if ui.button(sel_label).clicked() {
                        self.multi_select_mode = !self.multi_select_mode;
                        if !self.multi_select_mode {
                            self.selected_items.clear();
                        }
                    }

                    if !self.selected_items.is_empty() {
                        let list: Vec<_> = self.selected_items.iter().cloned().collect();
                        if ui.button("📋 Копир.").clicked() {
                            self.clipboard = Some((ClipboardOp::Copy, list.clone()));
                            self.multi_select_mode = false;
                            self.selected_items.clear();
                            self.status_toast = Some("Скопировано!".into());
                        }
                        if ui.button("✂ Перем.").clicked() {
                            self.clipboard = Some((ClipboardOp::Move, list.clone()));
                            self.multi_select_mode = false;
                            self.selected_items.clear();
                            self.status_toast = Some("Готово к переносу!".into());
                        }
                        if ui.button("📤 Отпр.").clicked() {
                            Self::share_paths(&list);
                        }
                        if ui.button("ℹ Свед.").clicked() {
                            self.info_targets = Some(list.clone());
                        }
                        if ui.button("🗑 Удал.").clicked() {
                            self.request_delete(list);
                        }
                    }
                }
            });

            // Капсулы вкладок Material 3 Expressive
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                if ui.selectable_label(self.active_tab == ActiveTab::Explorer, "📁 Папки").clicked() {
                    self.active_tab = ActiveTab::Explorer;
                    self.viewing_media = None;
                    self.img_editor = None;
                    self.vid_editor = None;
                    self.refresh_dir();
                    self.trigger_anim();
                }
                if ui.selectable_label(self.active_tab == ActiveTab::Gallery, "🖼 Галерея").clicked() {
                    self.active_tab = ActiveTab::Gallery;
                    self.viewing_media = None;
                    self.img_editor = None;
                    self.vid_editor = None;
                    self.scan_all_media();
                    self.trigger_anim();
                }
                if ui.selectable_label(self.active_tab == ActiveTab::Favorites, "⭐ Избранное").clicked() {
                    self.active_tab = ActiveTab::Favorites;
                    self.viewing_media = None;
                    self.img_editor = None;
                    self.vid_editor = None;
                    self.trigger_anim();
                }
                if ui.selectable_label(self.active_tab == ActiveTab::Trash, "🗑 Корзина").clicked() {
                    self.active_tab = ActiveTab::Trash;
                    self.viewing_media = None;
                    self.img_editor = None;
                    self.vid_editor = None;
                    self.trigger_anim();
                }
                if ui.selectable_label(self.active_tab == ActiveTab::Settings, "⚙ Настройки").clicked() {
                    self.active_tab = ActiveTab::Settings;
                    self.viewing_media = None;
                    self.img_editor = None;
                    self.vid_editor = None;
                    self.trigger_anim();
                }
            });
            ui.add_space(6.0);
        });

        // ОСНОВНАЯ ПАНЕЛЬ
        let (surface, surface_cont, _, _) = self.settings.m3_palette();
        let frame = egui::Frame::central_panel(&ctx.style()).fill(surface);

        egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
            // Пользовательский фото-фон
            if let Some(bg_path) = &self.settings.bg_image_path {
                let rect = ui.max_rect();
                let uri = format!("file://{}", bg_path);
                egui::Image::new(uri).paint_at(ui, rect);
                let alpha = (self.settings.bg_dimming * 255.0) as u8;
                ui.painter().rect_filled(
                    rect,
                    0.0,
                    egui::Color32::from_rgba_unmultiplied(0, 0, 0, alpha),
                );
            }

            // 1. ФОТОРЕДАКТОР RGE
            if let Some(ed) = &mut self.img_editor {
                ui.horizontal_wrapped(|ui| {
                    if ui.button("💾 Сохранить").clicked() {
                        match ed.apply_and_save(ctx) {
                            Ok(saved) => {
                                self.status_toast = Some(format!("Сохранено: {}", saved.display()));
                                self.viewing_media = Some(saved);
                                self.img_editor = None;
                            }
                            Err(e) => self.status_toast = Some(format!("Ошибка: {}", e)),
                        }
                        return;
                    }
                    if ui.button("↻ 90°").clicked() {
                        ed.rotation = (ed.rotation + 1) % 4;
                    }
                    if ui.button("⇄ Отразить").clicked() {
                        ed.flip_h = !ed.flip_h;
                    }
                    ui.checkbox(&mut ed.grayscale, "Ч/Б");
                    ui.checkbox(&mut ed.draw_mode, "✏ Кисть");
                });

                ui.horizontal(|ui| {
                    ui.label("Яркость:");
                    ui.add(egui::Slider::new(&mut ed.brightness, -80..=80));
                });
                ui.horizontal(|ui| {
                    ui.label("Контраст:");
                    ui.add(egui::Slider::new(&mut ed.contrast, -40.0..=40.0));
                });
                ui.horizontal(|ui| {
                    ui.label("Обрезка:");
                    ui.add(egui::Slider::new(&mut ed.crop_margin, 0.0..=0.4));
                });

                if ed.draw_mode {
                    ui.horizontal(|ui| {
                        ui.label("Цвет:");
                        ui.color_edit_button_srgba(&mut ed.brush_color);
                        ui.add(egui::Slider::new(&mut ed.brush_size, 2.0..=24.0));
                        if ui.button("Очистить").clicked() {
                            ed.strokes.clear();
                        }
                    });
                }

                let uri = format!("file://{}", ed.path.display());
                let avail = ui.available_size();
                let (response, painter) = ui.allocate_painter(avail, egui::Sense::drag());
                let rect = response.rect;

                let angle = (ed.rotation as f32) * std::f32::consts::FRAC_PI_2;
                egui::Image::new(uri)
                    .rotate(angle, egui::vec2(0.5, 0.5))
                    .paint_at(ui, rect);

                if ed.crop_margin > 0.01 {
                    let crop_rect = rect.shrink2(egui::vec2(
                        rect.width() * ed.crop_margin,
                        rect.height() * ed.crop_margin,
                    ));
                    painter.rect_stroke(crop_rect, 12.0, egui::Stroke::new(2.5, egui::Color32::YELLOW));
                }

                if ed.draw_mode {
                    if response.drag_started() {
                        ed.strokes.push((Vec::new(), ed.brush_color, ed.brush_size));
                    }
                    if let Some(pos) = response.interact_pointer_pos() {
                        if let Some(last) = ed.strokes.last_mut() {
                            last.0.push(pos);
                        }
                    }
                }
                for (pts, col, size) in &ed.strokes {
                    if pts.len() >= 2 {
                        painter.add(egui::Shape::line(pts.clone(), egui::Stroke::new(*size, *col)));
                    }
                }
                return;
            }

            // 2. ВИДЕОРЕДАКТОР RGE
            if let Some(ved) = &mut self.vid_editor {
                egui::Frame::none()
                    .fill(surface_cont)
                    .rounding(24.0)
                    .inner_margin(16.0)
                    .show(ui, |ui| {
                        ui.heading("🎬 Видеоредактор RGE");
                        ui.label(format!("Файл: {}", ved.path.file_name().unwrap_or_default().to_string_lossy()));
                        ui.separator();

                        ui.add(egui::Slider::new(&mut ved.start_sec, 0.0..=300.0).text("Начало обрезки (сек)"));
                        ui.add(egui::Slider::new(&mut ved.duration_sec, 1.0..=300.0).text("Длительность клипа (сек)"));
                        ui.add(egui::Slider::new(&mut ved.speed, 0.5..=2.0).text("Скорость воспроизведения"));

                        ui.horizontal(|ui| {
                            ui.label("Поворот:");
                            ui.radio_value(&mut ved.rotation, 0, "0°");
                            ui.radio_value(&mut ved.rotation, 90, "90°");
                            ui.radio_value(&mut ved.rotation, 180, "180°");
                            ui.radio_value(&mut ved.rotation, 270, "270°");
                        });

                        ui.checkbox(&mut ved.mute_audio, "🔇 Убрать звуковую дорожку");
                        ui.add_space(12.0);

                        ui.horizontal(|ui| {
                            if ui.button("💾 Экспортировать видео").clicked() {
                                match ved.export_video() {
                                    Ok(saved) => {
                                        self.status_toast = Some(format!("Видео сохранено: {}", saved.display()));
                                        self.viewing_media = Some(saved);
                                        self.vid_editor = None;
                                    }
                                    Err(msg) => {
                                        self.status_toast = Some(msg);
                                        self.vid_editor = None;
                                    }
                                }
                            }
                            if ui.button("Отмена").clicked() {
                                self.vid_editor = None;
                            }
                        });
                    });
                return;
            }

            // 3. ПОЛНОЭКРАННЫЙ ПРОСМОТР ФОТО И ВИДЕО
            if let Some(media_path) = self.viewing_media.clone() {
                ui.horizontal_wrapped(|ui| {
                    let is_fav = self.settings.favorites.contains(&media_path);
                    let fav_txt = if is_fav { "★ В избранном" } else { "☆ В избранное" };
                    if ui.button(fav_txt).clicked() {
                        if is_fav {
                            self.settings.favorites.remove(&media_path);
                        } else {
                            self.settings.favorites.insert(media_path.clone());
                        }
                        self.settings.save();
                    }

                    if Self::is_image(&media_path) {
                        if ui.button("🎨 Редактировать").clicked() {
                            self.img_editor = Some(ImageEditorState::new(media_path.clone()));
                        }
                        if ui.button("🖼 Фон RGE").clicked() {
                            self.settings.bg_image_path = Some(media_path.to_string_lossy().to_string());
                            self.settings.save();
                            self.status_toast = Some("Фон RGE обновлен!".into());
                        }
                    } else if Self::is_video(&media_path) {
                        if ui.button("✂ Редактор видео").clicked() {
                            self.vid_editor = Some(VideoEditorState::new(media_path.clone()));
                        }
                    }

                    if ui.button("📤 Отправить").clicked() {
                        Self::share_paths(&[media_path.clone()]);
                    }
                    if ui.button("📲 Использовать как...").clicked() {
                        let mime = if Self::is_video(&media_path) { "video/*" } else { "image/*" };
                        Self::android_intent("android.intent.action.ATTACH_DATA", &media_path, mime);
                    }
                    if ui.button("📋 Копировать").clicked() {
                        self.clipboard = Some((ClipboardOp::Copy, vec![media_path.clone()]));
                        self.status_toast = Some("Скопировано!".into());
                    }
                    if ui.button("✂ Переместить").clicked() {
                        self.clipboard = Some((ClipboardOp::Move, vec![media_path.clone()]));
                        self.status_toast = Some("Готово к переносу!".into());
                    }
                    if ui.button("ℹ Сведения").clicked() {
                        self.info_targets = Some(vec![media_path.clone()]);
                    }
                    if ui.button("🗑 Удалить").clicked() {
                        self.request_delete(vec![media_path.clone()]);
                    }
                });

                if let Some(toast) = &self.status_toast {
                    ui.colored_label(egui::Color32::LIGHT_GREEN, toast);
                }

                ui.separator();

                let anim = self.anim_factor();
                if Self::is_video(&media_path) {
                    ui.centered_and_justified(|ui| {
                        egui::Frame::none()
                            .fill(surface_cont)
                            .rounding(28.0)
                            .inner_margin(24.0)
                            .show(ui, |ui| {
                                ui.vertical_centered(|ui| {
                                    ui.heading(format!(
                                        "🎬 {}",
                                        media_path.file_name().unwrap_or_default().to_string_lossy()
                                    ));
                                    ui.add_space(16.0);
                                    if ui.button("▶ Воспроизвести со звуком").clicked() {
                                        Self::android_intent("android.intent.action.VIEW", &media_path, "video/*");
                                    }
                                });
                            });
                    });
                } else {
                    let uri = format!("file://{}", media_path.display());
                    let scale = 0.88 + 0.12 * anim;
                    ui.centered_and_justified(|ui| {
                        ui.add(
                            egui::Image::new(uri)
                                .rounding(18.0)
                                .max_size(ui.available_size() * scale)
                                .maintain_aspect_ratio(true),
                        );
                    });
                }
                return;
            }

            // 4. НАСТРОЙКИ MATERIAL 3 EXPRESSIVE
            if self.active_tab == ActiveTab::Settings {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    egui::Frame::none()
                        .fill(surface_cont)
                        .rounding(24.0)
                        .inner_margin(14.0)
                        .show(ui, |ui| {
                            ui.heading("🎨 Дизайн Material 3 Expressive");
                            ui.add(egui::Slider::new(&mut self.settings.ui_scale, 0.8..=2.2).text("Масштаб приложения"));
                            ui.add(egui::Slider::new(&mut self.settings.font_size, 12.0..=28.0).text("Размер текста"));
                            ui.add(egui::Slider::new(&mut self.settings.top_padding, 0.0..=90.0).text("Отступ статус-бара"));

                            ui.label("Палитра темы:");
                            ui.horizontal_wrapped(|ui| {
                                ui.radio_value(&mut self.settings.theme_seed, M3ThemeSeed::LavenderPurple, "Лаванда (Android 17)");
                                ui.radio_value(&mut self.settings.theme_seed, M3ThemeSeed::EmeraldMint, "Мята");
                                ui.radio_value(&mut self.settings.theme_seed, M3ThemeSeed::SunsetCoral, "Коралл");
                                ui.radio_value(&mut self.settings.theme_seed, M3ThemeSeed::OceanBlue, "Океан");
                                ui.radio_value(&mut self.settings.theme_seed, M3ThemeSeed::CustomColor, "Свой цвет");
                            });

                            if self.settings.theme_seed == M3ThemeSeed::CustomColor {
                                ui.horizontal(|ui| {
                                    ui.label("Свой цвет фона:");
                                    let mut color = egui::Color32::from_rgb(
                                        self.settings.custom_bg_color[0],
                                        self.settings.custom_bg_color[1],
                                        self.settings.custom_bg_color[2],
                                    );
                                    if ui.color_edit_button_srgba(&mut color).changed() {
                                        self.settings.custom_bg_color = [color.r(), color.g(), color.b()];
                                    }
                                });
                            }

                            if self.settings.bg_image_path.is_some() {
                                ui.horizontal(|ui| {
                                    if ui.button("Убрать фото-фон").clicked() {
                                        self.settings.bg_image_path = None;
                                    }
                                });
                                ui.add(egui::Slider::new(&mut self.settings.bg_dimming, 0.0..=0.95).text("Затемнение обоев"));
                            }
                        });

                    ui.add_space(10.0);

                    egui::Frame::none()
                        .fill(surface_cont)
                        .rounding(24.0)
                        .inner_margin(14.0)
                        .show(ui, |ui| {
                            ui.heading("📐 Сетка, анимации и корзина");
                            ui.horizontal(|ui| {
                                ui.label("Вид папок:");
                                ui.radio_value(&mut self.settings.explorer_view, ViewMode::List, "Список");
                                ui.radio_value(&mut self.settings.explorer_view, ViewMode::Grid, "Сетка");
                            });
                            ui.add(egui::Slider::new(&mut self.settings.grid_columns, 2..=6).text("Колонок в сетке"));

                            ui.separator();
                            ui.horizontal(|ui| {
                                ui.label("Анимация:");
                                ui.radio_value(&mut self.settings.anim_style, AnimStyle::ExpressiveSpring, "Пружина M3");
                                ui.radio_value(&mut self.settings.anim_style, AnimStyle::SmoothFade, "Плавная");
                                ui.radio_value(&mut self.settings.anim_style, AnimStyle::None, "Выкл");
                            });
                            ui.add(egui::Slider::new(&mut self.settings.anim_speed, 0.05..=1.0).text("Скорость анимации (сек)"));

                            ui.separator();
                            ui.label("При удалении файлов:");
                            ui.radio_value(&mut self.settings.delete_policy, DeletePolicy::AskEveryTime, "Спрашивать каждый раз");
                            ui.radio_value(&mut self.settings.delete_policy, DeletePolicy::MoveToTrash, "Сразу в корзину");
                            ui.radio_value(&mut self.settings.delete_policy, DeletePolicy::DeletePermanently, "Удалять навсегда");
                        });

                    ui.add_space(12.0);
                    if ui.button("💾 Сохранить настройки RGE").clicked() {
                        self.settings.save();
                        self.status_toast = Some("Настройки RGE сохранены!".into());
                    }
                    if let Some(t) = &self.status_toast {
                        ui.colored_label(egui::Color32::LIGHT_GREEN, t);
                    }
                });
                return;
            }

            // 5. ОСНОВНОЙ СПИСОК / СЕТКА С БЫСТРЫМИ МИНИАТЮРАМИ
            if let Some(t) = &self.status_toast {
                ui.horizontal(|ui| {
                    ui.colored_label(egui::Color32::LIGHT_GREEN, t);
                    if ui.button("✖").clicked() {
                        self.status_toast = None;
                    }
                });
            }

            let items_to_show: Vec<PathBuf> = match self.active_tab {
                ActiveTab::Explorer => {
                    ui.label(format!("📂 {}", self.current_dir.display()));
                    self.entries.clone()
                }
                ActiveTab::Gallery => {
                    ui.label(format!("Медиатека RGE: {} файлов", self.all_media.len()));
                    self.all_media.clone()
                }
                ActiveTab::Favorites => {
                    ui.label(format!("Избранное: {}", self.settings.favorites.len()));
                    self.settings.favorites.iter().cloned().collect()
                }
                ActiveTab::Trash => {
                    ui.horizontal(|ui| {
                        ui.label("Корзина:");
                        if ui.button("🔥 Очистить").clicked() {
                            let _ = fs::remove_dir_all(TRASH_DIR);
                            let _ = fs::create_dir_all(TRASH_DIR);
                        }
                    });
                    fs::read_dir(TRASH_DIR)
                        .map(|rd| rd.flatten().map(|e| e.path()).collect())
                        .unwrap_or_default()
                }
                ActiveTab::Settings => Vec::new(),
            };

            egui::ScrollArea::vertical().show(ui, |ui| {
                let use_grid = self.active_tab == ActiveTab::Gallery
                    || self.settings.explorer_view == ViewMode::Grid;

                if use_grid {
                    let cols = self.settings.grid_columns.max(2);
                    let cell_size = (ui.available_width() / cols as f32) - 8.0;

                    ui.horizontal_wrapped(|ui| {
                        for path in items_to_show {
                            let is_selected = self.selected_items.contains(&path);
                            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("?");

                            ui.vertical(|ui| {
                                ui.set_width(cell_size);
                                let clicked = if Self::is_image(&path) {
                                    // Используем легкую миниатюру для 60 FPS без лагов
                                    let thumb_uri = self.get_fast_thumbnail_uri(&path);
                                    let img = egui::Image::new(thumb_uri)
                                        .rounding(16.0)
                                        .fit_to_exact_size(egui::vec2(cell_size, cell_size - 26.0));
                                    ui.add(egui::ImageButton::new(img).selected(is_selected)).clicked()
                                } else {
                                    let icon = if path.is_dir() { "📁" } else if Self::is_video(&path) { "🎬" } else { "📄" };
                                    ui.add_sized(
                                        [cell_size, cell_size - 26.0],
                                        egui::SelectableLabel::new(is_selected, format!("{}
{}", icon, name)),
                                    )
                                    .clicked()
                                };

                                if clicked {
                                    self.handle_item_click(path.clone());
                                }
                                if ui.small_button("⋮ Действия").clicked() {
                                    self.action_target = Some(path);
                                }
                            });
                        }
                    });
                } else {
                    for path in items_to_show {
                        let is_selected = self.selected_items.contains(&path);
                        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("???");
                        let icon = if path.is_dir() {
                            "📁"
                        } else if Self::is_image(&path) {
                            "🖼"
                        } else if Self::is_video(&path) {
                            "🎬"
                        } else {
                            "📄"
                        };

                        let prefix = if self.multi_select_mode {
                            if is_selected { "☑ " } else { "☐ " }
                        } else {
                            ""
                        };

                        egui::Frame::none()
                            .fill(surface_cont)
                            .rounding(18.0)
                            .inner_margin(egui::Margin::symmetric(10.0, 6.0))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    if ui.selectable_label(is_selected, format!("{}{} {}", prefix, icon, name)).clicked() {
                                        self.handle_item_click(path.clone());
                                    }
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        if self.active_tab == ActiveTab::Trash {
                                            if ui.button("♻").clicked() {
                                                let dest = Path::new(ROOT_DIR).join(name);
                                                let _ = fs::rename(&path, dest);
                                            }
                                        } else if ui.button("⋮").clicked() {
                                            self.action_target = Some(path);
                                        }
                                    });
                                });
                            });
                    }
                }
            });
        });
    }
}

impl RgeApp {
    fn handle_item_click(&mut self, path: PathBuf) {
        if self.multi_select_mode {
            if self.selected_items.contains(&path) {
                self.selected_items.remove(&path);
            } else {
                self.selected_items.insert(path);
            }
            return;
        }

        if path.is_dir() {
            self.current_dir = path;
            self.refresh_dir();
            self.trigger_anim();
        } else if Self::is_image(&path) || Self::is_video(&path) {
            self.viewing_media = Some(path);
            self.trigger_anim();
        } else {
            self.action_target = Some(path);
        }
    }
}

#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(app: winit::platform::android::activity::AndroidApp) {
    use winit::platform::android::EventLoopBuilderExtAndroid;

    android_logger::init_once(
        android_logger::Config::default().with_max_level(log::LevelFilter::Info),
    );

    let mut options = NativeOptions::default();
    options.event_loop_builder = Some(Box::new(move |builder| {
        builder.with_android_app(app);
    }));

    eframe::run_native(
        "RGE",
        options,
        Box::new(|cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);
            Ok(Box::new(RgeApp::default()))
        }),
    )
    .unwrap();
}

use eframe::egui;
use std::fs;
use std::path::{Path, PathBuf};

#[cfg(target_os = "android")]
use eframe::NativeOptions;

struct ExplorerGalleryApp {
    current_dir: PathBuf,
    entries: Vec<PathBuf>,
    selected_image: Option<PathBuf>,
    gallery_mode: bool,
    error_msg: Option<String>,
}

impl Default for ExplorerGalleryApp {
    fn default() -> Self {
        // Стандартный путь к пользовательским файлам на Android
        let start_dir = PathBuf::from("/storage/emulated/0");
        let fallback_dir = PathBuf::from("/");
        let current_dir = if start_dir.exists() { start_dir } else { fallback_dir };

        let mut app = Self {
            current_dir,
            entries: Vec::new(),
            selected_image: None,
            gallery_mode: false,
            error_msg: None,
        };
        app.refresh_dir();
        app
    }
}

impl ExplorerGalleryApp {
    fn refresh_dir(&mut self) {
        self.entries.clear();
        self.error_msg = None;

        match fs::read_dir(&self.current_dir) {
            Ok(read_dir) => {
                let mut dirs = Vec::new();
                let mut files = Vec::new();

                for entry in read_dir.flatten() {
                    let path = entry.path();
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
            Err(e) => {
                self.error_msg = Some(format!("Нет доступа к папке: {}", e));
            }
        }
    }

    fn is_image(path: &Path) -> bool {
        if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
            matches!(ext.to_lowercase().as_str(), "jpg" | "jpeg" | "png" | "webp" | "gif")
        } else {
            false
        }
    }
}

impl eframe::App for ExplorerGalleryApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Верхняя панель управления
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.add_space(25.0); // Отступ под статус-бар Android
            ui.horizontal(|ui| {
                if ui.button("⬅ Назад").clicked() {
                    if self.selected_image.is_some() {
                        self.selected_image = None;
                    } else if let Some(parent) = self.current_dir.parent() {
                        self.current_dir = parent.to_path_buf();
                        self.refresh_dir();
                    }
                }

                ui.separator();

                let mode_text = if self.gallery_mode { "📁 Список" } else { "🖼 Галерея" };
                if ui.button(mode_text).clicked() {
                    self.gallery_mode = !self.gallery_mode;
                }
            });
            ui.label(format!("Путь: {}", self.current_dir.display()));
            ui.add_space(5.0);
        });

        // Основная область
        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(err) = &self.error_msg {
                ui.colored_label(egui::Color32::RED, err);
                ui.label("Разрешите доступ к файлам в настройках приложения на телефоне.");
                return;
            }

            // 1. Режим полноэкранного просмотра фото
            if let Some(img_path) = &self.selected_image.clone() {
                let uri = format!("file://{}", img_path.display());
                ui.centered_and_justified(|ui| {
                    ui.add(
                        egui::Image::new(uri)
                            .max_size(ui.available_size())
                            .maintain_aspect_ratio(true),
                    );
                });
                return;
            }

            // 2. Режим галереи или проводника
            egui::ScrollArea::vertical().show(ui, |ui| {
                if self.gallery_mode {
                    // Сетка изображений
                    ui.horizontal_wrapped(|ui| {
                        for path in &self.entries.clone() {
                            if path.is_file() && Self::is_image(path) {
                                let uri = format!("file://{}", path.display());
                                let img = egui::Image::new(uri)
                                    .fit_to_exact_size(egui::vec2(110.0, 110.0));
                                
                                if ui.add(egui::ImageButton::new(img)).clicked() {
                                    self.selected_image = Some(path.clone());
                                }
                            }
                        }
                    });
                } else {
                    // Обычный список файлов и папок
                    for path in &self.entries.clone() {
                        let name = path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("???");

                        let is_dir = path.is_dir();
                        let icon = if is_dir { "📁" } else if Self::is_image(path) { "🖼" } else { "📄" };
                        let label = format!("{} {}", icon, name);

                        if ui.selectable_label(false, label).clicked() {
                            if is_dir {
                                self.current_dir = path.clone();
                                self.refresh_dir();
                            } else if Self::is_image(path) {
                                self.selected_image = Some(path.clone());
                            }
                        }
                    }
                }
            });
        });
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
        "Rust Explorer",
        options,
        Box::new(|cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);
            Ok(Box::new(ExplorerGalleryApp::default()))
        }),
    )
    .unwrap();
}

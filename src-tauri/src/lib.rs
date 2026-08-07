// 初始化 i18n，设置 fallback 语言为中文
rust_i18n::i18n!("locales", fallback = "zh");

pub mod commands;
mod i18n;
pub mod logging;
pub mod models;
pub mod security;
pub mod services;

use commands::security::{get_scan_results, scan_all_installed_skills};
use commands::AppState;
use services::{Database, PluginManager, SkillManager};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder};
use tauri::Manager;
use tokio::sync::Mutex;

const MAIN_WINDOW_LABEL: &str = "main";
const MENU_SHOW: &str = "show";
const MENU_HIDE: &str = "hide";
const MENU_QUIT: &str = "quit";

#[cfg(target_os = "macos")]
const MACOS_TRAY_TEMPLATE_ICON: tauri::image::Image<'static> =
    tauri::include_image!("icons/tray-icon-template.png");

#[cfg(target_os = "macos")]
fn maybe_suppress_macos_os_activity_logs() {
    // macOS 在某些场景会输出类似：
    // CoreText note: Client requested name ".SFNS-..."
    // 这类日志通常来自系统/依赖层（如 WebKit），对应用功能无影响，但会污染开发期控制台输出。
    //
    // 默认：在 macOS 下抑制（避免影响用户/开发者体验）。
    // 如需强制开启（不抑制），可显式设置 OS_ACTIVITY_MODE（例如：`OS_ACTIVITY_MODE=default`）。
    if std::env::var_os("OS_ACTIVITY_MODE").is_some() {
        return;
    }

    // SAFETY: 仅从 main() 在启动 Tauri 运行时之前调用，此时尚未创建 Tokio 工作线程
    unsafe { std::env::set_var("OS_ACTIVITY_MODE", "disable") };
}

/// 获取托盘菜单文本（中英文双语）
///
/// 返回值：(显示窗口文本, 隐藏窗口文本, 退出文本)
fn get_menu_texts() -> (&'static str, &'static str, &'static str) {
    // 简化版：使用中英文双语显示
    ("显示 / Show", "隐藏 / Hide", "退出 / Quit")
}

fn create_tray_menu(app: &tauri::AppHandle) -> Result<tauri::menu::Menu<tauri::Wry>, tauri::Error> {
    let (show_text, hide_text, quit_text) = get_menu_texts();

    let show_item = MenuItemBuilder::with_id(MENU_SHOW, show_text).build(app)?;
    let hide_item = MenuItemBuilder::with_id(MENU_HIDE, hide_text).build(app)?;
    let quit_item = MenuItemBuilder::with_id(MENU_QUIT, quit_text).build(app)?;

    MenuBuilder::new(app)
        .item(&show_item)
        .item(&hide_item)
        .separator()
        .item(&quit_item)
        .build()
}

fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        if let Err(e) = window.show() {
            log::warn!("显示窗口失败: {}", e);
        }
        if let Err(e) = window.unminimize() {
            log::warn!("取消最小化失败: {}", e);
        }
        if let Err(e) = window.set_focus() {
            log::warn!("设置窗口焦点失败: {}", e);
        }
    } else {
        log::error!("无法获取主窗口");
    }
}

fn handle_menu_event(app: &tauri::AppHandle, event: tauri::menu::MenuEvent) {
    log::debug!("菜单事件: {}", event.id().as_ref());

    match event.id().as_ref() {
        MENU_SHOW => {
            show_main_window(app);
        }
        MENU_HIDE => {
            if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
                if let Err(e) = window.hide() {
                    log::warn!("隐藏窗口失败: {}", e);
                }
            } else {
                log::error!("无法获取主窗口");
            }
        }
        MENU_QUIT => {
            log::info!("用户通过托盘菜单退出应用");
            app.exit(0);
        }
        _ => {
            log::warn!("未知的菜单事件: {}", event.id().as_ref());
        }
    }
}

fn handle_tray_event(tray: &tauri::tray::TrayIcon<tauri::Wry>, event: tauri::tray::TrayIconEvent) {
    if let tauri::tray::TrayIconEvent::Click {
        button: MouseButton::Left,
        button_state: MouseButtonState::Up,
        ..
    } = event
    {
        log::debug!("托盘图标被点击");
        let app = tray.app_handle();
        if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
            match window.is_visible() {
                Ok(true) => {
                    if let Err(e) = window.hide() {
                        log::warn!("隐藏窗口失败: {}", e);
                    }
                }
                Ok(false) => {
                    show_main_window(app);
                }
                Err(e) => {
                    log::error!("检查窗口可见性失败: {}", e);
                }
            }
        } else {
            log::error!("无法获取主窗口");
        }
    }
}

fn ensure_cli_path() {
    let mut paths: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|value| std::env::split_paths(&value).collect())
        .unwrap_or_default();

    let mut candidates: Vec<PathBuf> = Vec::new();

    #[cfg(target_os = "macos")]
    {
        candidates.extend(
            [
                "/opt/homebrew/bin",
                "/usr/local/bin",
                "/usr/bin",
                "/bin",
                "/usr/sbin",
                "/sbin",
            ]
            .iter()
            .map(PathBuf::from),
        );
    }

    #[cfg(target_os = "linux")]
    {
        candidates.extend(
            ["/usr/local/bin", "/usr/bin", "/bin", "/usr/sbin", "/sbin"]
                .iter()
                .map(PathBuf::from),
        );
    }

    if let Some(home) = dirs::home_dir() {
        candidates.push(home.join(".local").join("bin"));
        candidates.push(home.join(".npm-global").join("bin"));
        candidates.push(home.join(".npm").join("bin"));
        candidates.push(home.join(".claude").join("bin"));
        candidates.push(home.join("bin"));

        // nvm: 扫描 ~/.nvm/versions/node/*/bin
        let nvm_versions = home.join(".nvm").join("versions").join("node");
        if nvm_versions.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&nvm_versions) {
                for entry in entries.flatten() {
                    let bin_path = entry.path().join("bin");
                    if bin_path.is_dir() {
                        candidates.push(bin_path);
                    }
                }
            }
        }

        // fnm (macOS)
        #[cfg(target_os = "macos")]
        {
            let fnm_versions = home
                .join("Library")
                .join("Application Support")
                .join("fnm")
                .join("node-versions");
            if fnm_versions.is_dir() {
                if let Ok(entries) = std::fs::read_dir(&fnm_versions) {
                    for entry in entries.flatten() {
                        let bin_path = entry.path().join("installation").join("bin");
                        if bin_path.is_dir() {
                            candidates.push(bin_path);
                        }
                    }
                }
            }
        }

        // fnm (Linux)
        #[cfg(target_os = "linux")]
        {
            let fnm_versions = home
                .join(".local")
                .join("share")
                .join("fnm")
                .join("node-versions");
            if fnm_versions.is_dir() {
                if let Ok(entries) = std::fs::read_dir(&fnm_versions) {
                    for entry in entries.flatten() {
                        let bin_path = entry.path().join("installation").join("bin");
                        if bin_path.is_dir() {
                            candidates.push(bin_path);
                        }
                    }
                }
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            let appdata = PathBuf::from(appdata);
            candidates.push(appdata.join("npm"));
            candidates.push(appdata.join("nvm"));
        }
        if let Some(local_appdata) = std::env::var_os("LOCALAPPDATA") {
            candidates.push(PathBuf::from(local_appdata).join("Programs").join("nodejs"));
        }
        if let Some(program_files) = std::env::var_os("ProgramFiles") {
            candidates.push(PathBuf::from(program_files).join("nodejs"));
        }
        if let Some(program_files_x86) = std::env::var_os("ProgramFiles(x86)") {
            candidates.push(PathBuf::from(program_files_x86).join("nodejs"));
        }
        if let Some(program_data) = std::env::var_os("ProgramData") {
            candidates.push(PathBuf::from(program_data).join("chocolatey").join("bin"));
        }
        if let Some(userprofile) = std::env::var_os("USERPROFILE") {
            candidates.push(PathBuf::from(userprofile).join("scoop").join("shims"));
        }
        if let Some(nvm_home) = std::env::var_os("NVM_HOME") {
            candidates.push(PathBuf::from(nvm_home));
        }
        if let Some(nvm_symlink) = std::env::var_os("NVM_SYMLINK") {
            candidates.push(PathBuf::from(nvm_symlink));
        }
    }

    let mut added: Vec<PathBuf> = Vec::new();
    for candidate in candidates {
        if !candidate.exists() {
            continue;
        }
        if paths.iter().any(|p| p == &candidate) {
            continue;
        }
        paths.push(candidate.clone());
        added.push(candidate);
    }

    if added.is_empty() {
        return;
    }

    match std::env::join_paths(paths) {
        Ok(joined) => {
            // SAFETY: 仅由 init() 从 main() 在 Tauri/Tokio 运行时启动前调用，此时尚无其他线程
            unsafe { std::env::set_var("PATH", joined) };
            let list = added
                .iter()
                .map(|p| p.to_string_lossy())
                .collect::<Vec<_>>()
                .join(", ");
            log::info!("已扩展 PATH，新增: {}", list);
        }
        Err(err) => {
            log::warn!("扩展 PATH 失败: {}", err);
        }
    }
}

/// 应用启动前的环境初始化（必须在 Tokio 运行时启动前调用）
pub fn init() {
    #[cfg(target_os = "macos")]
    maybe_suppress_macos_os_activity_logs();

    // 初始化日志（stderr + 滚动文件）并安装 panic 钩子。
    // 必须最先执行：启动期的 panic 正是最需要留下现场的场景。
    logging::init();
    ensure_cli_path();
}

/// 为损坏的数据库生成带时间戳的备份路径
fn corrupt_backup_path(db_path: &Path) -> PathBuf {
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let file_name = db_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "agent-skills.db".to_string());
    db_path.with_file_name(format!("{file_name}.corrupt-{stamp}"))
}

/// 在数据库文件路径后追加后缀，得到 WAL / SHM 边车文件路径。
///
/// 用 `OsString` 拼接而非 `format!("{}", path.display())`：`Display` 是给人看的
/// 有损格式化，对非 UTF-8 路径不保证能还原出等价路径。
fn sidecar_path(db_path: &Path, suffix: &str) -> PathBuf {
    let mut raw = db_path.as_os_str().to_os_string();
    raw.push(suffix);
    PathBuf::from(raw)
}

/// 判断错误是否表明「数据库文件本身已损坏」，而非环境问题。
///
/// 只有这一类错误才适合走「备份 + 重建空库」——那意味着原文件已经无法读取，
/// 保留它也没有意义。而 BUSY（被其他进程占用）、权限不足、磁盘满、
/// CANTOPEN 等属于环境问题：原库很可能完好，重建会造成**真实的数据丢失**。
fn is_database_corrupted(error: &anyhow::Error) -> bool {
    for cause in error.chain() {
        if let Some(sqlite_error) = cause.downcast_ref::<rusqlite::Error>() {
            if let rusqlite::Error::SqliteFailure(ffi_error, _) = sqlite_error {
                if matches!(
                    ffi_error.code,
                    rusqlite::ErrorCode::DatabaseCorrupt | rusqlite::ErrorCode::NotADatabase
                ) {
                    return true;
                }
            }
        }
    }
    false
}

/// 把损坏的数据库连同 WAL / SHM 边车文件一起移开，为重建腾出位置。
///
/// 返回备份路径；备份失败则返回 `None`，由调用方判定为不可恢复。
fn move_aside_corrupt_database(db_path: &Path) -> Option<PathBuf> {
    if !db_path.exists() {
        return None;
    }

    let backup = corrupt_backup_path(db_path);
    match std::fs::rename(db_path, &backup) {
        Ok(()) => {
            log::warn!("已将损坏的数据库备份到: {}", backup.display());
        }
        Err(e) => {
            log::error!("备份损坏数据库失败 {}: {}", db_path.display(), e);
            return None;
        }
    }

    // WAL / SHM 属于旧数据库，留下会污染新建的库
    for suffix in ["-wal", "-shm"] {
        let sidecar = sidecar_path(db_path, suffix);
        if sidecar.exists() {
            if let Err(e) = std::fs::remove_file(&sidecar) {
                log::warn!("清理 {} 失败: {}", sidecar.display(), e);
            }
        }
    }

    Some(backup)
}

/// 打开数据库；**仅当文件本身已损坏**时，才备份原文件并重建一次。
///
/// 此前这里是 `Database::new(db_path).expect(...)`，数据库损坏、文件被锁定、
/// 权限异常都会让进程在启动瞬间 panic 退出，正是「打开即闪退」的典型成因。
///
/// 重建是有代价的操作（用户丢失全部历史记录），因此触发条件必须收紧：
/// 只认 SQLITE_CORRUPT / SQLITE_NOTADB。锁占用、权限、磁盘满等环境问题下
/// 原库通常完好，此时如实报错让用户去解决，远好过悄悄清空数据。
fn open_database_with_recovery(db_path: &Path) -> Result<(Database, Option<PathBuf>), String> {
    let open_error = match Database::new(db_path.to_path_buf()) {
        Ok(db) => return Ok((db, None)),
        Err(e) => e,
    };

    if !is_database_corrupted(&open_error) {
        log::error!("打开数据库失败（非文件损坏，保留原库）: {:#}", open_error);
        return Err(format!(
            "无法打开数据库：{open_error:#}\n\n\
             数据库文件已保留。常见原因：另一个实例正在运行、文件被占用、\
             权限不足或磁盘空间不足。"
        ));
    }

    log::error!("数据库文件已损坏，尝试备份并重建: {:#}", open_error);

    let Some(backup) = move_aside_corrupt_database(db_path) else {
        return Err(format!(
            "数据库文件已损坏且无法备份原文件：{open_error:#}"
        ));
    };

    Database::new(db_path.to_path_buf())
        .map(|db| {
            log::warn!("已使用空数据库重建，历史数据保留在备份文件中");
            (db, Some(backup))
        })
        .map_err(|e| format!("重建数据库仍然失败: {e:#}"))
}

/// 组装带日志路径的用户可读错误文案
fn fatal_startup_detail(message: &str) -> String {
    match logging::log_file_path() {
        Some(path) => format!("{message}\n\n日志文件：{}", path.display()),
        None => message.to_string(),
    }
}

/// 启动失败时弹出原生对话框，让用户知道发生了什么并能找到日志。
/// 需要已有 `AppHandle`，因此只能在 `setup` 之后调用。
fn show_fatal_startup_dialog(app: &tauri::AppHandle, message: &str) {
    use tauri_plugin_dialog::{DialogExt, MessageDialogKind};

    app.dialog()
        .message(fatal_startup_detail(message))
        .kind(MessageDialogKind::Error)
        .title("Agent Skills Guard 启动失败")
        .blocking_show();
}

/// 数据库损坏并被重建后告知用户。
///
/// 这不是致命错误（应用可以正常使用），但用户的技能 / 仓库记录全部清空了 ——
/// 只写一条 `log::warn` 等于让用户在毫不知情的情况下面对一个空应用。
fn show_database_rebuilt_dialog(app: &tauri::AppHandle, backup: &Path) {
    use tauri_plugin_dialog::{DialogExt, MessageDialogKind};

    app.dialog()
        .message(format!(
            "检测到数据库文件已损坏，已自动重建。\n\n\
             此前的技能与仓库记录需要重新扫描或添加。\n\
             原始文件已备份至：\n{}",
            backup.display()
        ))
        .kind(MessageDialogKind::Warning)
        .title("数据库已重建")
        .blocking_show();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            // 应用数据目录：拿不到或建不出来都是不可恢复的，但要给出可读提示
            // 并留下日志，而不是 panic 后静默消失。
            let app_dir = match app.path().app_data_dir() {
                Ok(dir) => dir,
                Err(e) => {
                    let msg = format!("无法定位应用数据目录: {e}");
                    show_fatal_startup_dialog(app.handle(), &msg);
                    return Err(msg.into());
                }
            };

            if let Err(e) = std::fs::create_dir_all(&app_dir) {
                let msg = format!("无法创建应用数据目录 {}: {e}", app_dir.display());
                show_fatal_startup_dialog(app.handle(), &msg);
                return Err(msg.into());
            }

            let db_path = app_dir.join("agent-skills.db");

            // 初始化数据库（损坏时自动备份并重建）
            let (db, corrupt_backup) = match open_database_with_recovery(&db_path) {
                Ok(result) => result,
                Err(e) => {
                    let msg = format!("数据库初始化失败: {e}");
                    show_fatal_startup_dialog(app.handle(), &msg);
                    return Err(msg.into());
                }
            };

            if let Some(backup) = corrupt_backup {
                // 数据已重建，应用可正常使用，但用户的历史记录已清空 ——
                // 必须明确告知，并给出备份路径
                log::warn!("数据库曾损坏并已重建，原文件备份于: {}", backup.display());
                show_database_rebuilt_dialog(app.handle(), &backup);
            }

            let db = Arc::new(db);

            let migration_manager = services::MigrationManager::new(Arc::clone(&db));
            match migration_manager.run_startup_migrations() {
                Ok(summary) => {
                    if summary.discovered > 0 || summary.created > 0 || summary.updated > 0 {
                        log::info!(
                            "Startup skill adoption completed: discovered={}, created={}, updated={}",
                            summary.discovered,
                            summary.created,
                            summary.updated
                        );
                    }
                }
                Err(error) => {
                    log::warn!("Startup skill adoption failed: {}", error);
                }
            }

            // 初始化 SkillManager
            let skill_manager = SkillManager::new(Arc::clone(&db));
            let skill_manager = Arc::new(Mutex::new(skill_manager));

            // 初始化 PluginManager
            let plugin_manager = PluginManager::new(Arc::clone(&db));
            let plugin_manager = Arc::new(Mutex::new(plugin_manager));

            // 初始化 GitHub 服务
            let github = Arc::new(services::GitHubService::new());

            // 设置应用状态
            app.manage(AppState {
                db,
                skill_manager,
                plugin_manager,
                github,
                cli_scan_cache: std::sync::RwLock::new(None),
            });

            // 初始化系统托盘
            let icon = {
                #[cfg(target_os = "macos")]
                {
                    MACOS_TRAY_TEMPLATE_ICON.clone()
                }

                #[cfg(not(target_os = "macos"))]
                {
                    app.default_window_icon()
                        .ok_or("无法获取默认窗口图标")?
                        .clone()
                }
            };

            let app_handle = app.handle();
            let menu = create_tray_menu(&app_handle)?;

            let tray = TrayIconBuilder::new()
                .icon(icon)
                .icon_as_template(cfg!(target_os = "macos"))
                .tooltip("Agent Skills Guard")
                .menu(&menu)
                .on_tray_icon_event(handle_tray_event)
                .on_menu_event(handle_menu_event)
                .build(app)?;

            // 存储托盘实例到 app state
            app.manage(tray);

            // 监听窗口关闭请求，改为隐藏到托盘
            if let Some(main_window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
                let app_handle = app.handle().clone();
                main_window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        log::info!("窗口关闭请求，隐藏到托盘而不是退出");
                        // 阻止默认关闭行为
                        api.prevent_close();
                        // 隐藏窗口而不是关闭
                        if let Some(window) = app_handle.get_webview_window(MAIN_WINDOW_LABEL) {
                            if let Err(e) = window.hide() {
                                log::error!("隐藏窗口失败: {}", e);
                            }
                        } else {
                            log::error!("无法获取主窗口");
                        }
                    }
                });
            } else {
                log::warn!("无法获取主窗口，窗口关闭监听器未设置");
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::add_repository,
            commands::get_repositories,
            commands::delete_repository,
            commands::scan_repository,
            commands::get_skills,
            commands::get_installed_skills,
            commands::install_skill,
            commands::prepare_skill_installation,
            commands::confirm_skill_installation,
            commands::cancel_skill_installation,
            commands::uninstall_skill,
            commands::uninstall_skill_path,
            commands::delete_skill,
            commands::scan_local_skills,
            commands::refresh_skill_links,
            commands::clear_repository_cache,
            commands::clear_all_repository_caches,
            commands::refresh_repository_cache,
            commands::get_cache_stats,
            commands::open_skill_directory,
            commands::get_default_install_path,
            commands::select_custom_install_path,
            commands::get_featured_repositories,
            commands::refresh_featured_repositories,
            commands::import_featured_repositories,
            commands::featured_marketplaces::get_featured_marketplaces,
            commands::featured_marketplaces::refresh_featured_marketplaces,
            commands::reset_app_data,
            commands::is_repository_added,
            commands::check_skills_updates,
            commands::prepare_skill_update,
            commands::confirm_skill_update,
            commands::cancel_skill_update,
            commands::auto_scan_unscanned_repositories,
            commands::list_agent_tools,
            commands::sync_skill_to_tools,
            commands::sync_all_skills_to_tools,
            commands::local_cli::list_local_cli_tools,
            commands::local_cli::rescan_local_cli_tools,
            commands::local_cli::check_local_cli_updates,
            commands::local_cli::update_local_cli_tool,
            commands::local_cli::uninstall_local_cli_tool,
            commands::local_cli::open_local_cli_folder,
            commands::local_cli::fetch_local_cli_descriptions,
            commands::plugins::get_plugins,
            commands::plugins::get_plugins_cached,
            commands::plugins::sync_featured_marketplace_plugins,
            commands::plugins::prepare_plugin_installation,
            commands::plugins::confirm_plugin_installation,
            commands::plugins::cancel_plugin_installation,
            commands::plugins::uninstall_plugin,
            commands::plugins::remove_marketplace,
            commands::plugins::get_claude_marketplaces,
            commands::plugins::check_plugins_updates,
            commands::plugins::update_plugin,
            commands::plugins::check_marketplaces_updates,
            commands::plugins::update_marketplace,
            commands::plugins::get_skill_plugin_upgrade_candidates,
            commands::plugins::scan_all_installed_plugins,
            commands::plugins::scan_installed_plugin,
            scan_all_installed_skills,
            commands::security::scan_installed_skill,
            commands::security::count_scan_files,
            get_scan_results,
        ])
        .build(tauri::generate_context!());

    // 构建失败（含 setup 返回的错误）时优雅退出：panic 钩子已记录现场，
    // 这里再补一条明确的致命错误并以非零码退出，而不是 panic 出一堆噪声。
    let app = match app {
        Ok(app) => app,
        Err(e) => {
            log::error!("{}", fatal_startup_detail(&format!("应用启动失败: {e}")));
            log::logger().flush();
            std::process::exit(1);
        }
    };

    app.run(|_app_handle, _event| {
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = _event {
                log::info!("收到 macOS Reopen 事件，尝试恢复主窗口");
                show_main_window(_app_handle);
            }
        });
}

#[cfg(test)]
mod tests {
    use super::{
        corrupt_backup_path, is_database_corrupted, open_database_with_recovery, sidecar_path,
    };
    use std::path::Path;

    #[test]
    fn sidecar_path_appends_suffix_without_lossy_formatting() {
        let db = Path::new("/tmp/dir/agent-skills.db");
        assert_eq!(
            sidecar_path(db, "-wal"),
            Path::new("/tmp/dir/agent-skills.db-wal")
        );
        assert_eq!(
            sidecar_path(db, "-shm"),
            Path::new("/tmp/dir/agent-skills.db-shm")
        );
    }

    #[test]
    fn corrupt_backup_path_keeps_file_next_to_original() {
        let db = Path::new("/tmp/dir/agent-skills.db");
        let backup = corrupt_backup_path(db);
        assert_eq!(backup.parent(), db.parent());
        let name = backup.file_name().unwrap().to_string_lossy().into_owned();
        assert!(
            name.starts_with("agent-skills.db.corrupt-"),
            "unexpected backup name: {name}"
        );
    }

    #[test]
    fn only_corruption_codes_trigger_rebuild() {
        let corrupt = anyhow::Error::from(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CORRUPT),
            None,
        ));
        assert!(is_database_corrupted(&corrupt));

        let not_a_db = anyhow::Error::from(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_NOTADB),
            None,
        ));
        assert!(is_database_corrupted(&not_a_db));

        // 环境类错误：原库很可能完好，重建会造成真实的数据丢失
        for code in [
            rusqlite::ffi::SQLITE_BUSY,
            rusqlite::ffi::SQLITE_PERM,
            rusqlite::ffi::SQLITE_FULL,
            rusqlite::ffi::SQLITE_CANTOPEN,
            rusqlite::ffi::SQLITE_READONLY,
        ] {
            let err = anyhow::Error::from(rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(code),
                None,
            ));
            assert!(
                !is_database_corrupted(&err),
                "sqlite code {code} must not be treated as corruption"
            );
        }
    }

    /// 通过 `anyhow::Context` 包装后仍需能识别底层 sqlite 错误码，
    /// 因为 `Database::new` 全程使用 `.context(...)`。
    #[test]
    fn corruption_is_detected_through_context_chain() {
        use anyhow::Context;

        let wrapped: anyhow::Result<()> = Err(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_NOTADB),
            None,
        ))
        .context("Failed to open database");

        assert!(is_database_corrupted(&wrapped.unwrap_err()));
    }

    #[test]
    fn garbage_file_is_backed_up_and_rebuilt() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("agent-skills.db");
        // 非 SQLite 内容 → SQLITE_NOTADB
        std::fs::write(&db_path, b"this is definitely not a sqlite database").unwrap();

        let (db, backup) = open_database_with_recovery(&db_path).expect("should rebuild");

        let backup = backup.expect("corrupt file must be backed up");
        assert!(backup.exists(), "backup file should exist");
        assert!(db_path.exists(), "a fresh database should be in place");
        // 重建后的库可用
        assert_eq!(db.get_repositories().unwrap().len(), 0);
    }

    #[test]
    fn healthy_database_is_opened_without_backup() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("agent-skills.db");

        let (_db, backup) = open_database_with_recovery(&db_path).expect("fresh db opens");
        assert!(backup.is_none(), "a healthy database must not be backed up");

        // 二次打开同样不应触发备份
        let (_db, backup) = open_database_with_recovery(&db_path).expect("existing db opens");
        assert!(
            backup.is_none(),
            "reopening an existing database must not back it up"
        );
        assert_eq!(
            std::fs::read_dir(dir.path())
                .unwrap()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_name().to_string_lossy().contains(".corrupt-"))
                .count(),
            0,
            "no corrupt backups should be created for a healthy database"
        );
    }
}

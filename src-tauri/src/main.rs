// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // 环境初始化（日志、PATH）必须在 Tauri/Tokio 运行时启动前完成
    agent_skills_guard_lib::init();
    agent_skills_guard_lib::run();
}

// Без чёрного консольного окна в релизе
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};

/// Области (в физических пикселях, от левого верхнего угла окна), где мышь «ловится» плеером.
/// head — только голова (по ней открывается панель), any — голова + открытая панель.
#[derive(Default)]
struct Hit {
    head: Mutex<Vec<[f64; 4]>>,
    any: Mutex<Vec<[f64; 4]>>,
}

#[tauri::command]
fn set_rects(state: State<'_, Hit>, head: Vec<[f64; 4]>, any: Vec<[f64; 4]>) {
    *state.head.lock().unwrap() = head;
    *state.any.lock().unwrap() = any;
}

#[tauri::command]
fn quit(app: AppHandle) {
    app.exit(0);
}

/// Открывает страницу входа Spotify в системном браузере (только accounts.spotify.com).
#[tauri::command]
fn open_url(url: String) -> Result<(), String> {
    if !url.starts_with("https://accounts.spotify.com/") {
        return Err("blocked".into());
    }
    #[cfg(target_os = "windows")]
    std::process::Command::new("rundll32")
        .args(["url.dll,FileProtocolHandler", &url])
        .spawn()
        .map_err(|e| e.to_string())?;
    #[cfg(target_os = "macos")]
    std::process::Command::new("open")
        .arg(&url)
        .spawn()
        .map_err(|e| e.to_string())?;
    #[cfg(target_os = "linux")]
    std::process::Command::new("xdg-open")
        .arg(&url)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Крошечный сервер на 127.0.0.1:8888: ловит возврат из Spotify после входа
/// и передаёт параметры (code=...) в плеер.
fn serve_callback(app: AppHandle) {
    let listener = match TcpListener::bind("127.0.0.1:8888") {
        Ok(l) => l,
        Err(_) => return,
    };
    for stream in listener.incoming() {
        let Ok(mut s) = stream else { continue };
        let mut buf = [0u8; 4096];
        let n = s.read(&mut buf).unwrap_or(0);
        let req = String::from_utf8_lossy(&buf[..n]).to_string();
        let line = req.lines().next().unwrap_or("");
        if let Some(q) = line
            .split_whitespace()
            .nth(1)
            .and_then(|p| p.strip_prefix("/callback?"))
        {
            let _ = app.emit("sp-callback", q.to_string());
        }
        let body = "<meta charset=utf-8><title>Chibi Player</title><body style=\"font-family:sans-serif;background:#121212;color:#fff;display:grid;place-items:center;height:100vh;margin:0\"><h2>Готово! Можно закрыть вкладку и вернуться в плеер.</h2></body>";
        let _ = write!(
            s,
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
    }
}

fn main() {
    tauri::Builder::default()
        .manage(Hit::default())
        .invoke_handler(tauri::generate_handler![set_rects, quit, open_url])
        .setup(|app| {
            let win = app.get_webview_window("main").expect("main window");

            // Окно на весь основной монитор (на 1 пиксель ниже, чтобы Windows не считал его «полноэкранным»)
            if let Ok(Some(m)) = win.primary_monitor() {
                let _ = win.set_position(*m.position());
                let _ = win.set_size(tauri::PhysicalSize::new(
                    m.size().width,
                    m.size().height.saturating_sub(1),
                ));
            }
            // Пока курсор не над плеером — все клики проходят «сквозь» окно
            let _ = win.set_ignore_cursor_events(true);

            let h = app.handle().clone();
            thread::spawn(move || serve_callback(h));

            let h2 = app.handle().clone();
            let w2 = win.clone();
            thread::spawn(move || {
                let mut last = (false, false);
                loop {
                    // редко проверяем, пока курсор далеко; чаще — когда рядом
                    thread::sleep(Duration::from_millis(if last.1 { 30 } else { 80 }));
                    let (Ok(c), Ok(p)) = (w2.cursor_position(), w2.outer_position()) else {
                        continue;
                    };
                    let (cx, cy) = (c.x - p.x as f64, c.y - p.y as f64);
                    let st = h2.state::<Hit>();
                    let inside = |v: &[[f64; 4]]| {
                        v.iter()
                            .any(|r| cx >= r[0] && cx <= r[0] + r[2] && cy >= r[1] && cy <= r[1] + r[3])
                    };
                    let cur = (inside(&st.head.lock().unwrap()[..]), inside(&st.any.lock().unwrap()[..]));
                    if cur != last {
                        let _ = w2.set_ignore_cursor_events(!cur.1);
                        let _ = h2.emit("hover", format!("{},{}", cur.0 as u8, cur.1 as u8));
                        last = cur;
                    }
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("ошибка запуска Chibi Player");
}

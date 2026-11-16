// Без чёрного консольного окна в релизе
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};
use tauri::{
    AppHandle, Manager, PhysicalPosition, PhysicalSize, State, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};

/// Размер окна со Spotify (логические пиксели)
const PANEL_W: f64 = 540.0;
const PANEL_H: f64 = 720.0;

/// Скрипт внутри Spotify: сообщает голове, играет ли музыка (чтобы она крутилась).
const INIT_JS: &str = r#"(function(){
  if (window.top !== window) return;
  var last = null;
  setInterval(function(){
    try {
      var p = !!(navigator.mediaSession && navigator.mediaSession.playbackState === 'playing');
      if (p !== last) {
        last = p;
        if (window.__TAURI_INTERNALS__) {
          window.__TAURI_INTERNALS__.invoke('plugin:event|emit', { event: 'sp-playing', payload: p });
        }
      }
    } catch (e) {}
  }, 1000);
})();"#;

/// Клик по кнопке «пауза / играть» в самом Spotify
const TOGGLE_JS: &str = r#"(function(){
  var b = document.querySelector('[data-testid="control-button-playpause"]');
  if (b) b.click();
})();"#;

/// Области (физические пиксели от левого верхнего угла прозрачного окна), которые ловят мышь.
#[derive(Default)]
struct Hit {
    head: Mutex<Vec<[f64; 4]>>,
    any: Mutex<Vec<[f64; 4]>>,
    drag: AtomicBool,
}

#[tauri::command]
fn set_rects(state: State<'_, Hit>, head: Vec<[f64; 4]>, any: Vec<[f64; 4]>) {
    *state.head.lock().unwrap() = head;
    *state.any.lock().unwrap() = any;
}

#[tauri::command]
fn set_drag(state: State<'_, Hit>, on: bool) {
    state.drag.store(on, Ordering::Relaxed);
}

#[tauri::command]
fn toggle_play(app: AppHandle) {
    if let Some(w) = app.get_webview_window("panel") {
        let _ = w.eval(TOGGLE_JS);
    }
}

#[tauri::command]
fn quit(app: AppHandle) {
    app.exit(0);
}

/// Окно со Spotify создаётся при первом наведении на голову и дальше живёт скрытым (музыка не прерывается).
fn ensure_panel(app: &AppHandle) -> Option<WebviewWindow> {
    if let Some(w) = app.get_webview_window("panel") {
        return Some(w);
    }
    let url: tauri::Url = "https://open.spotify.com".parse().ok()?;
    WebviewWindowBuilder::new(app, "panel", WebviewUrl::External(url))
        .title("Spotify")
        .inner_size(PANEL_W, PANEL_H)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .visible(false)
        .focused(false)
        .initialization_script(INIT_JS)
        .build()
        .ok()
}

fn inside(v: &[[f64; 4]], x: f64, y: f64, pad: f64) -> bool {
    v.iter()
        .any(|r| x >= r[0] - pad && x <= r[0] + r[2] + pad && y >= r[1] - pad && y <= r[1] + r[3] + pad)
}

/// Ставит окно Spotify рядом с головой (справа или слева, где хватает места).
fn place_panel(w: &WebviewWindow, head: [f64; 4], ox: f64, oy: f64, mon: (f64, f64, f64, f64), sf: f64) {
    let pw = (PANEL_W * sf).min(mon.2 - 16.0);
    let ph = (PANEL_H * sf).min(mon.3 - 16.0);
    let hx = head[0] + ox;
    let hy = head[1] + oy;
    let right_x = hx + head[2] - 6.0 * sf;
    let x = if right_x + pw <= mon.0 + mon.2 - 8.0 { right_x } else { hx - pw + 6.0 * sf };
    let x = x.max(mon.0 + 8.0).min(mon.0 + mon.2 - pw - 8.0);
    let y = (hy + head[3] / 2.0 - ph / 2.0)
        .max(mon.1 + 8.0)
        .min(mon.1 + mon.3 - ph - 8.0);
    let _ = w.set_size(PhysicalSize::new(pw as u32, ph as u32));
    let _ = w.set_position(PhysicalPosition::new(x as i32, y as i32));
}

fn main() {
    tauri::Builder::default()
        .manage(Hit::default())
        .invoke_handler(tauri::generate_handler![set_rects, set_drag, toggle_play, quit])
        .setup(|app| {
            let overlay = app.get_webview_window("main").expect("main window");

            // Прозрачное окно на весь основной монитор (на 1 пиксель ниже, чтобы Windows не считал его «полноэкранным»)
            let mut mon = (0.0_f64, 0.0_f64, 1920.0_f64, 1080.0_f64);
            if let Ok(Some(m)) = overlay.primary_monitor() {
                mon = (
                    m.position().x as f64,
                    m.position().y as f64,
                    m.size().width as f64,
                    m.size().height as f64,
                );
                let _ = overlay.set_position(*m.position());
                let _ = overlay.set_size(PhysicalSize::new(
                    m.size().width,
                    m.size().height.saturating_sub(1),
                ));
            }
            // Пока курсор не над головой — все клики проходят «сквозь» окно
            let _ = overlay.set_ignore_cursor_events(true);
            let sf = overlay.scale_factor().unwrap_or(1.0);

            let h2 = app.handle().clone();
            thread::spawn(move || {
                let mut last_any = false;
                let mut shown = false;
                let mut leave: Option<Instant> = None;
                loop {
                    // редко проверяем, пока курсор далеко; чаще — когда рядом
                    thread::sleep(Duration::from_millis(if shown || last_any { 30 } else { 80 }));
                    let (Ok(c), Ok(p)) = (overlay.cursor_position(), overlay.outer_position()) else {
                        continue;
                    };
                    let (ox, oy) = (p.x as f64, p.y as f64);
                    let (rx, ry) = (c.x - ox, c.y - oy);

                    let st = h2.state::<Hit>();
                    let head: Option<[f64; 4]> = st.head.lock().unwrap().first().copied();
                    let in_any = inside(&st.any.lock().unwrap()[..], rx, ry, 0.0);
                    let dragging = st.drag.load(Ordering::Relaxed);
                    let in_head = match head {
                        Some(r) => inside(&[r], rx, ry, 14.0 * sf),
                        None => false,
                    };

                    if in_any != last_any {
                        let _ = overlay.set_ignore_cursor_events(!in_any);
                        last_any = in_any;
                    }

                    // во время перетаскивания головы Spotify прячем
                    if dragging {
                        if shown {
                            if let Some(w) = h2.get_webview_window("panel") {
                                let _ = w.hide();
                            }
                            shown = false;
                        }
                        leave = None;
                        continue;
                    }

                    let panel = h2.get_webview_window("panel");
                    let in_panel = match (&panel, shown) {
                        (Some(w), true) => match (w.outer_position(), w.outer_size()) {
                            (Ok(pp), Ok(sz)) => {
                                c.x >= pp.x as f64
                                    && c.x <= pp.x as f64 + sz.width as f64
                                    && c.y >= pp.y as f64
                                    && c.y <= pp.y as f64 + sz.height as f64
                            }
                            _ => false,
                        },
                        _ => false,
                    };

                    if in_head && !shown {
                        if let (Some(w), Some(hd)) = (ensure_panel(&h2), head) {
                            place_panel(&w, hd, ox, oy, mon, sf);
                            let _ = w.show();
                            shown = true;
                            leave = None;
                        }
                    } else if shown {
                        if in_head || in_panel {
                            leave = None;
                        } else {
                            match leave {
                                None => leave = Some(Instant::now()),
                                Some(t) => {
                                    if t.elapsed() > Duration::from_millis(450) {
                                        if let Some(w) = &panel {
                                            let _ = w.hide();
                                        }
                                        shown = false;
                                        leave = None;
                                    }
                                }
                            }
                        }
                    }
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("ошибка запуска Chibi Player");
}

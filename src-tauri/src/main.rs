use cadence_core::{
    Active, ActiveSession, DaySummary, Engine, EntryInput, ImportPreview, Mode, Phase, Result,
    Session, SessionInput, Stamp, Stint,
};
use chrono::Local;
use serde::Serialize;
use std::{collections::BTreeMap, io::Write, sync::Mutex, time::Instant};
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Emitter, Manager, State,
};
use tauri_plugin_notification::NotificationExt;

struct AppState {
    engine: Mutex<Engine>,
    origin: Instant,
    pending_import: Mutex<Option<String>>,
    warning: Mutex<Option<String>>,
}
impl AppState {
    fn now(&self) -> Stamp {
        Stamp {
            mono_ms: self.origin.elapsed().as_millis() as u64,
            wall: Local::now().fixed_offset(),
        }
    }
    fn engine(&self) -> Result<std::sync::MutexGuard<'_, Engine>> {
        self.engine
            .lock()
            .map_err(|_| "Storage lock failed. Restart cadence.".into())
    }
}
#[derive(Serialize, Clone)]
struct View {
    active: Option<Active>,
    active_session: Option<ActiveSession>,
    warning: Option<String>,
}
fn notify_done(app: &tauri::AppHandle) {
    if let Err(e) = app
        .notification()
        .builder()
        .title("Stint ended")
        .body("Review and save the ended stint in cadence.")
        .show()
    {
        report(app, format!("Could not show desktop notification: {e}"));
    }
}
fn report(app: &tauri::AppHandle, message: String) {
    eprintln!("cadence: {message}");
    if let Some(state) = app.try_state::<AppState>() {
        if let Ok(mut warning) = state.warning.lock() {
            *warning = Some(message.clone());
        }
    }
    let _ = app.emit("cadence-error", message);
}
fn show(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}
fn publish(app: &tauri::AppHandle, state: &AppState) -> Result<View> {
    let mut engine = state.engine()?;
    let now = state.now();
    if engine.tick(&now)? {
        notify_done(app);
    }
    let view = View {
        active: engine.snapshot(&now),
        active_session: engine.session_snapshot(&now),
        warning: state.warning.lock().map_err(|e| e.to_string())?.clone(),
    };
    app.emit("timer-state", &view).map_err(|e| e.to_string())?;
    Ok(view)
}
#[tauri::command]
fn get_state(app: tauri::AppHandle, state: State<AppState>) -> Result<View> {
    publish(&app, &state)
}
#[tauri::command]
fn start_stint(
    app: tauri::AppHandle,
    state: State<AppState>,
    mode: Mode,
    label: String,
    minutes: Option<u64>,
) -> Result<View> {
    state.engine()?.start(mode, label, minutes, &state.now())?;
    publish(&app, &state)
}
#[tauri::command]
fn timer_action(app: tauri::AppHandle, state: State<AppState>, action: String) -> Result<View> {
    {
        let mut e = state.engine()?;
        let now = state.now();
        if e.tick(&now)? {
            notify_done(&app);
        }
        match action.as_str() {
            "pause" => e.pause(&now)?,
            "resume" => e.resume(&now)?,
            "finish" => e.finish(&now)?,
            "discard" => e.discard()?,
            _ => return Err("Unknown timer action.".into()),
        }
    }
    publish(&app, &state)
}
#[tauri::command]
fn save_stint(
    app: tauri::AppHandle,
    state: State<AppState>,
    label: String,
    description: String,
) -> Result<Stint> {
    let s = state.engine()?.save(label, description)?;
    publish(&app, &state)?;
    Ok(s)
}
#[tauri::command]
fn start_session(
    app: tauri::AppHandle,
    state: State<AppState>,
    input: SessionInput,
) -> Result<View> {
    state.engine()?.start_session(input, &state.now())?;
    publish(&app, &state)
}
#[tauri::command]
fn session_action(app: tauri::AppHandle, state: State<AppState>, action: String) -> Result<View> {
    {
        let mut engine = state.engine()?;
        let now = state.now();
        match action.as_str() {
            "pause" => engine.pause_session(&now)?,
            "resume" => engine.resume_session(&now)?,
            "end" => engine.end_session(&now)?,
            "discard" => engine.discard_session()?,
            _ => return Err("Unknown session action.".into()),
        }
    }
    publish(&app, &state)
}
#[tauri::command]
fn save_session(
    app: tauri::AppHandle,
    state: State<AppState>,
    input: SessionInput,
) -> Result<Session> {
    let session = state.engine()?.save_session(input)?;
    publish(&app, &state)?;
    Ok(session)
}
#[tauri::command]
fn list_stints(state: State<AppState>) -> Result<Vec<Stint>> {
    state.engine()?.list()
}
#[tauri::command]
fn list_sessions(state: State<AppState>) -> Result<Vec<Session>> {
    state.engine()?.list_sessions()
}
#[tauri::command]
fn daily_summaries(state: State<AppState>) -> Result<BTreeMap<String, DaySummary>> {
    state.engine()?.summaries()
}
#[tauri::command]
fn edit_stint(state: State<AppState>, id: Option<String>, input: EntryInput) -> Result<Stint> {
    state.engine()?.upsert(id, input, &state.now())
}
#[tauri::command]
fn delete_stint(state: State<AppState>, id: String) -> Result<()> {
    state.engine()?.delete(&id)
}
#[tauri::command]
fn edit_session(state: State<AppState>, id: String, input: SessionInput) -> Result<Session> {
    state.engine()?.update_session(&id, input)
}
#[tauri::command]
fn delete_session(state: State<AppState>, id: String) -> Result<()> {
    state.engine()?.delete_session(&id)
}
#[tauri::command]
fn export_file(state: State<AppState>, path: String, format: String) -> Result<()> {
    let data = {
        let e = state.engine()?;
        match format.as_str() {
            "csv" => e.csv()?,
            "json" => e.backup()?,
            _ => return Err("Unknown export format.".into()),
        }
    };
    // Write beside the destination, then atomically replace it; preserve the old file on failure.
    let destination = std::path::Path::new(&path);
    let parent = destination.parent().ok_or("Invalid export path.")?;
    let temp = parent.join(format!(".cadence-export-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| -> Result<()> {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|e| e.to_string())?;
        f.write_all(data.as_bytes())
            .and_then(|_| f.sync_all())
            .map_err(|e| e.to_string())?;
        std::fs::rename(&temp, destination).map_err(|e| e.to_string())?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temp);
    }
    result
}
#[tauri::command]
fn preview_import(state: State<AppState>, path: String) -> Result<ImportPreview> {
    *state.pending_import.lock().map_err(|e| e.to_string())? = None;
    let meta = std::fs::metadata(&path).map_err(|e| e.to_string())?;
    if meta.len() > 50_000_000 {
        return Err("Backup exceeds the 50 MB import limit.".into());
    }
    let data = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let preview = state.engine()?.preview(&data)?;
    *state.pending_import.lock().map_err(|e| e.to_string())? = Some(data);
    Ok(preview)
}
#[tauri::command]
fn confirm_import(state: State<AppState>) -> Result<ImportPreview> {
    let mut pending = state.pending_import.lock().map_err(|e| e.to_string())?;
    let data = pending.as_ref().ok_or("Select a backup first.")?;
    let result = state.engine()?.import(data)?;
    *pending = None;
    Ok(result)
}
fn pause_for_exit(app: &tauri::AppHandle) -> Result<()> {
    let state = app.state::<AppState>();
    let mut e = state.engine()?;
    let now = state.now();
    if e.active_session.is_some() {
        e.pause_session(&now)?;
    } else if e.active.is_some() {
        e.pause(&now)?;
    }
    Ok(())
}
fn watch_suspend(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        let result = (|| -> std::result::Result<(), Box<dyn std::error::Error>> {
            let connection = zbus::blocking::Connection::system()?;
            let proxy = zbus::blocking::Proxy::new(
                &connection,
                "org.freedesktop.login1",
                "/org/freedesktop/login1",
                "org.freedesktop.login1.Manager",
            )?;
            let inhibit = || {
                proxy.call::<_, _, zbus::zvariant::OwnedFd>(
                    "Inhibit",
                    &(
                        "sleep",
                        "cadence",
                        "Persist and pause active timing",
                        "delay",
                    ),
                )
            };
            let mut signals = proxy.receive_signal("PrepareForSleep")?;
            let mut inhibitor = Some(inhibit()?);
            for signal in &mut signals {
                let (sleeping,): (bool,) = signal.body().deserialize()?;
                if sleeping {
                    if let Err(e) = pause_for_exit(&app) {
                        report(&app, e);
                    }
                    let _ = publish(&app, &app.state::<AppState>());
                    drop(inhibitor.take());
                } else {
                    inhibitor = Some(inhibit()?);
                }
            }
            Err("Suspend monitor disconnected".into())
        })();
        if let Err(e) = result {
            report(
                &app,
                format!(
                    "Automatic suspend pause is unavailable: {e}. Pause timing before suspending."
                ),
            );
        }
    });
}
fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| show(app)))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let directory = app.path().app_data_dir()?;
            std::fs::create_dir_all(&directory)?;
            let engine =
                Engine::open(directory.join("cadence.sqlite3")).map_err(std::io::Error::other)?;
            app.manage(AppState {
                engine: Mutex::new(engine),
                origin: Instant::now(),
                pending_import: Mutex::new(None),
                warning: Mutex::new(None),
            });
            let open = MenuItem::with_id(app, "open", "Open cadence", true, None::<&str>)?;
            let toggle =
                MenuItem::with_id(app, "toggle", "Pause / resume timing", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit cadence", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open, &toggle, &quit])?;
            TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("cadence — stint and session timer")
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open" => show(app),
                    "toggle" => {
                        let state = app.state::<AppState>();
                        let result = (|| -> Result<()> {
                            let mut e = state.engine()?;
                            let now = state.now();
                            if e.tick(&now)? {
                                notify_done(app);
                            }
                            match e.active.as_ref().map(|a| a.phase) {
                                Some(Phase::Running)
                                    if e.active_session
                                        .as_ref()
                                        .is_some_and(|s| s.phase == Phase::Running) =>
                                {
                                    e.pause_session(&now)?
                                }
                                Some(Phase::Paused)
                                    if e.active_session
                                        .as_ref()
                                        .is_some_and(|s| s.phase == Phase::Paused) =>
                                {
                                    e.resume_session(&now)?;
                                    e.resume(&now)?;
                                }
                                Some(Phase::Running) => e.pause(&now)?,
                                Some(Phase::Paused) => e.resume(&now)?,
                                _ => match e.active_session.as_ref().map(|s| s.phase) {
                                    Some(Phase::Running) => e.pause_session(&now)?,
                                    Some(Phase::Paused) => e.resume_session(&now)?,
                                    _ => show(app),
                                },
                            };
                            Ok(())
                        })();
                        if let Err(e) = result {
                            report(app, e);
                        }
                        let _ = publish(app, &state);
                    }
                    "quit" => match pause_for_exit(app) {
                        Ok(()) => app.exit(0),
                        Err(e) => {
                            show(app);
                            report(app, e);
                        }
                    },
                    _ => (),
                })
                .build(app)?;
            let handle = app.handle().clone();
            watch_suspend(handle.clone());
            std::thread::spawn(move || {
                let mut checkpoint = Instant::now();
                let mut last_error = String::new();
                loop {
                    std::thread::sleep(std::time::Duration::from_millis(250));
                    let state = handle.state::<AppState>();
                    let result = (|| -> Result<()> {
                        publish(&handle, &state)?;
                        if checkpoint.elapsed().as_secs() >= 5 {
                            state.engine()?.checkpoint(&state.now())?;
                            checkpoint = Instant::now();
                        }
                        Ok(())
                    })();
                    if let Err(e) = result {
                        if e != last_error {
                            report(&handle, e.clone());
                            last_error = e;
                        }
                    } else {
                        last_error.clear();
                    }
                }
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                if let Err(e) = window.hide() {
                    report(window.app_handle(), e.to_string());
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_state,
            start_stint,
            timer_action,
            save_stint,
            start_session,
            session_action,
            save_session,
            list_stints,
            list_sessions,
            daily_summaries,
            edit_stint,
            delete_stint,
            edit_session,
            delete_session,
            export_file,
            preview_import,
            confirm_import
        ])
        .build(tauri::generate_context!())
        .expect("Could not start cadence")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                if let Err(e) = pause_for_exit(app) {
                    api.prevent_exit();
                    show(app);
                    report(app, e);
                }
            }
        });
}

/// Tracks the last focused non-Madera toplevel window via zcosmic_toplevel_info_v1
/// and re-activates it before paste via zcosmic_toplevel_manager_v1.
///
/// This is the Linux equivalent of Windows' SetForegroundWindow() pattern already
/// used in paste_snippet_item / paste_history_item for Win32.
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use wayland_client::{
    delegate_noop,
    globals::{registry_queue_init, GlobalListContents},
    protocol::{wl_registry, wl_seat::WlSeat},
    Connection, Dispatch, QueueHandle,
};

use cosmic_protocols::toplevel_info::v1::client::{
    zcosmic_toplevel_handle_v1::{self, ZcosmicToplevelHandleV1},
    zcosmic_toplevel_info_v1::{self, ZcosmicToplevelInfoV1},
};
use cosmic_protocols::toplevel_management::v1::client::zcosmic_toplevel_manager_v1::{
    self, ZcosmicToplevelManagerV1,
};

// State value for "activated" in zcosmic_toplevel_handle_v1 protocol
const STATE_ACTIVATED: u32 = 2;

// ----- Shared state between the wayland thread and the rest of the app -----

pub struct SharedFocus {
    /// Continuously updated: last activated non-Madera window
    pub handle: Option<ZcosmicToplevelHandleV1>,
    /// Snapshot taken when the QuickPaste panel opens — THIS is what we restore.
    /// Without this, clicking the panel shifts focus to the window behind it,
    /// which gets stored as "last active" and we'd paste into the wrong window.
    pub snapshot: Option<ZcosmicToplevelHandleV1>,
    pub manager: Option<ZcosmicToplevelManagerV1>,
    pub seat: Option<WlSeat>,
}

static FOCUS: OnceLock<Arc<Mutex<SharedFocus>>> = OnceLock::new();

pub fn init() {
    let shared = Arc::new(Mutex::new(SharedFocus {
        handle: None,
        snapshot: None,
        manager: None,
        seat: None,
    }));
    let _ = FOCUS.set(shared.clone());
    std::thread::spawn(move || {
        run_wayland_thread(shared);
    });
}

/// Call this when the QuickPaste panel opens.
/// Snapshots the currently active window so we can restore it on paste,
/// regardless of what COSMIC does to focus after the panel appears.
pub fn snapshot_for_paste() {
    if let Some(focus) = FOCUS.get() {
        if let Ok(mut guard) = focus.lock() {
            guard.snapshot = guard.handle.clone();
            eprintln!("[focus_tracker] snapshot_for_paste: has_handle={}", guard.snapshot.is_some());
        }
    }
}

/// Activate the snapshotted window (captured when the panel opened).
/// Returns true if a window was activated.
pub fn activate_last_focused() -> bool {
    let focus = match FOCUS.get() {
        Some(f) => f,
        None => return false,
    };
    let guard = match focus.lock() {
        Ok(g) => g,
        Err(_) => return false,
    };
    // Use snapshot — the window that was active BEFORE the panel opened
    let handle = guard.snapshot.as_ref().or(guard.handle.as_ref());
    eprintln!("[focus_tracker] activate_last_focused: snapshot={} manager={} seat={}",
        guard.snapshot.is_some(), guard.manager.is_some(), guard.seat.is_some());
    match (handle, &guard.manager, &guard.seat) {
        (Some(_handle), Some(manager), Some(seat)) => {
            manager.activate(_handle, seat);
            eprintln!("[focus_tracker] activate sent!");
            true
        }
        _ => {
            eprintln!("[focus_tracker] activate SKIPPED - missing handle/manager/seat");
            false
        }
    }
}

// ----- Wayland thread internals -----

struct ToplevelMeta {
    app_id: String,
}

struct AppState {
    toplevels: HashMap<ZcosmicToplevelHandleV1, ToplevelMeta>,
    shared: Arc<Mutex<SharedFocus>>,
}

fn run_wayland_thread(shared: Arc<Mutex<SharedFocus>>) {
    eprintln!("[focus_tracker] thread started");
    let conn = match Connection::connect_to_env() {
        Ok(c) => { eprintln!("[focus_tracker] wayland connected"); c }
        Err(e) => { eprintln!("[focus_tracker] wayland connect FAILED: {e}"); return; }
    };

    let (globals, mut queue) = match registry_queue_init::<AppState>(&conn) {
        Ok(x) => { eprintln!("[focus_tracker] registry init OK"); x }
        Err(e) => { eprintln!("[focus_tracker] registry init FAILED: {e}"); return; }
    };

    let qh = queue.handle();

    // Bind the three globals we need
    let _info: Option<ZcosmicToplevelInfoV1> = globals.bind(&qh, 1..=3, ()).ok();
    let manager: Option<ZcosmicToplevelManagerV1> = globals.bind(&qh, 1..=4, ()).ok();
    let seat: Option<WlSeat> = globals.bind(&qh, 1..=9, ()).ok();
    eprintln!("[focus_tracker] info={} manager={} seat={}", _info.is_some(), manager.is_some(), seat.is_some());

    if let Ok(mut guard) = shared.lock() {
        guard.manager = manager;
        guard.seat = seat;
    }

    let mut state = AppState {
        toplevels: HashMap::new(),
        shared,
    };

    // Flush so COSMIC receives our bind requests and sends back existing toplevels
    if let Err(e) = queue.roundtrip(&mut state) {
        eprintln!("[focus_tracker] roundtrip failed: {e}");
        return;
    }
    eprintln!("[focus_tracker] roundtrip done, {} toplevels tracked", state.toplevels.len());

    loop {
        if queue.blocking_dispatch(&mut state).is_err() {
            break;
        }
    }
    eprintln!("[focus_tracker] dispatch loop ended");
}

// ----- Protocol dispatch implementations -----

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for AppState {
    fn event(
        _state: &mut Self,
        _proxy: &wl_registry::WlRegistry,
        _event: wl_registry::Event,
        _data: &GlobalListContents,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZcosmicToplevelInfoV1, ()> for AppState {
    fn event(
        state: &mut Self,
        _proxy: &ZcosmicToplevelInfoV1,
        event: zcosmic_toplevel_info_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        if let zcosmic_toplevel_info_v1::Event::Toplevel { toplevel } = event {
            state.toplevels.insert(toplevel, ToplevelMeta { app_id: String::new() });
        }
    }
}

impl Dispatch<ZcosmicToplevelHandleV1, ()> for AppState {
    fn event(
        state: &mut Self,
        handle: &ZcosmicToplevelHandleV1,
        event: zcosmic_toplevel_handle_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            zcosmic_toplevel_handle_v1::Event::AppId { app_id } => {
                if let Some(meta) = state.toplevels.get_mut(handle) {
                    meta.app_id = app_id;
                }
            }
            zcosmic_toplevel_handle_v1::Event::State { state: raw } => {
                let is_activated = raw
                    .chunks_exact(4)
                    .map(|b| u32::from_ne_bytes(b.try_into().unwrap_or([0; 4])))
                    .any(|v| v == STATE_ACTIVATED);

                if is_activated {
                    if let Some(meta) = state.toplevels.get(handle) {
                        eprintln!("[focus_tracker] activated: app_id={:?}", meta.app_id);
                        // Ignore our own windows
                        if !meta.app_id.contains("madera") {
                            if let Ok(mut guard) = state.shared.lock() {
                                guard.handle = Some(handle.clone());
                                eprintln!("[focus_tracker] stored as last_active");
                            }
                        }
                    }
                }
            }
            zcosmic_toplevel_handle_v1::Event::Closed => {
                state.toplevels.remove(handle);
                // Clear stored handle if this window closed
                if let Ok(mut guard) = state.shared.lock() {
                    if guard.handle.as_ref() == Some(handle) {
                        guard.handle = None;
                    }
                }
            }
            _ => {}
        }
    }
}

delegate_noop!(AppState: ignore ZcosmicToplevelManagerV1);
delegate_noop!(AppState: ignore WlSeat);

//! Getting capture bytes into the app: the native/web file dialog and drag-and-drop.

use std::sync::{Arc, Mutex};

/// Something delivered to the app from outside the UI thread.
pub enum Incoming {
    File { name: String, bytes: Vec<u8> },
    Error(String),
}

/// Shared mailbox the async file dialog drops its result in.
#[derive(Clone, Default)]
pub struct Inbox(Arc<Mutex<Vec<Incoming>>>);

impl Inbox {
    pub fn push(&self, item: Incoming) {
        if let Ok(mut q) = self.0.lock() {
            q.push(item);
        }
    }

    pub fn drain(&self) -> Vec<Incoming> {
        self.0.lock().map(|mut q| std::mem::take(&mut *q)).unwrap_or_default()
    }
}

/// Shows the platform file picker and delivers the chosen file to `inbox`.
pub fn open_dialog(inbox: &Inbox, ctx: &egui::Context) {
    let inbox = inbox.clone();
    let ctx = ctx.clone();
    spawn(async move {
        let picked = rfd::AsyncFileDialog::new()
            .add_filter("Replay captures", &["vrd"])
            .set_title("Open replay capture")
            .pick_file()
            .await;
        if let Some(file) = picked {
            let name = file.file_name();
            let bytes = file.read().await;
            inbox.push(Incoming::File { name, bytes });
            ctx.request_repaint();
        }
    });
}

#[cfg(not(target_arch = "wasm32"))]
fn spawn(future: impl std::future::Future<Output = ()> + Send + 'static) {
    std::thread::spawn(move || pollster::block_on(future));
}

#[cfg(target_arch = "wasm32")]
fn spawn(future: impl std::future::Future<Output = ()> + 'static) {
    wasm_bindgen_futures::spawn_local(future);
}

/// Reads a dropped file, from memory (web) or from disk (native).
pub fn read_dropped(file: &egui::DroppedFile) -> Incoming {
    if let Some(bytes) = &file.bytes {
        return Incoming::File { name: file.name.clone(), bytes: bytes.to_vec() };
    }
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(path) = &file.path {
        return read_path(path);
    }
    Incoming::Error(format!("Could not read {}", if file.name.is_empty() { "the dropped file" } else { &file.name }))
}

#[cfg(not(target_arch = "wasm32"))]
pub fn read_path(path: &std::path::Path) -> Incoming {
    let name = path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned());
    match std::fs::read(path) {
        Ok(bytes) => Incoming::File { name, bytes },
        Err(e) => Incoming::Error(format!("Cannot read {}: {e}", path.display())),
    }
}

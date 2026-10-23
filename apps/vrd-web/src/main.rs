//! Browser entry point. Build and serve it with `trunk serve` from this directory.
//!
//! Open `index.html?url=sample.vrd` to load a capture over HTTP at startup.

#[cfg(target_arch = "wasm32")]
mod web {
    use eframe::wasm_bindgen::JsCast as _;
    use vrd_ui_egui::{Inbox, Incoming, ReplayApp};
    use wasm_bindgen::JsValue;
    use wasm_bindgen_futures::JsFuture;

    struct WebApp(ReplayApp);

    impl eframe::App for WebApp {
        fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
            self.0.ui(ctx);
        }
    }

    pub fn run() {
        wasm_bindgen_futures::spawn_local(async {
            let document = eframe::web_sys::window().and_then(|w| w.document()).expect("no document");
            let canvas = document
                .get_element_by_id("vrd_canvas")
                .expect("index.html must contain a canvas with id `vrd_canvas`")
                .dyn_into::<eframe::web_sys::HtmlCanvasElement>()
                .expect("`vrd_canvas` is not a canvas");

            let started = eframe::WebRunner::new()
                .start(
                    canvas,
                    eframe::WebOptions::default(),
                    Box::new(|cc| {
                        let mut app = ReplayApp::new();
                        if let Some(state) = cc.wgpu_render_state.clone() {
                            app.attach_wgpu(state);
                        }
                        if let Some(url) = url_param() {
                            wasm_bindgen_futures::spawn_local(fetch_into(url, app.inbox(), cc.egui_ctx.clone()));
                        }
                        Ok(Box::new(WebApp(app)))
                    }),
                )
                .await;

            if let Some(loading) = document.get_element_by_id("loading") {
                match started {
                    Ok(()) => loading.remove(),
                    Err(e) => loading.set_inner_html(&format!("The debugger failed to start: {e:?}")),
                }
            }
        });
    }

    fn url_param() -> Option<String> {
        let search = web_sys::window()?.location().search().ok()?;
        web_sys::UrlSearchParams::new_with_str(&search).ok()?.get("url")
    }

    async fn fetch_into(url: String, inbox: Inbox, ctx: egui::Context) {
        let item = match fetch(&url).await {
            Ok(bytes) => Incoming::File { name: url.rsplit('/').next().unwrap_or(&url).to_string(), bytes },
            Err(e) => {
                Incoming::Error(format!("Cannot load {url}: {}", e.as_string().unwrap_or_else(|| format!("{e:?}"))))
            }
        };
        inbox.push(item);
        ctx.request_repaint();
    }

    async fn fetch(url: &str) -> Result<Vec<u8>, JsValue> {
        let window = web_sys::window().ok_or("no window")?;
        let response: web_sys::Response = JsFuture::from(window.fetch_with_str(url)).await?.dyn_into()?;
        if !response.ok() {
            return Err(JsValue::from_str(&format!("HTTP {}", response.status())));
        }
        let buffer = JsFuture::from(response.array_buffer()?).await?;
        Ok(js_sys::Uint8Array::new(&buffer).to_vec())
    }
}

#[cfg(target_arch = "wasm32")]
fn main() {
    web::run();
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!("vrd-web targets the browser. Run `trunk serve` in apps/vrd-web, or use the `vrd` app for desktop.");
}

//! WebMCP: offers the debugger's tools to AI agents running in the browser.
//!
//! Browsers that implement [WebMCP](https://webmachinelearning.github.io/webmcp/) expose
//! `document.modelContext` (earlier previews: `navigator.modelContext`). Each tool is registered there with
//! a JSON schema and an `execute` function. The tools themselves live in `vrd-ui-egui`'s `Bridge`; this
//! file is only the JavaScript glue.

use js_sys::{Function, JSON, Promise, Reflect};
use serde_json::{Value, json};
use vrd_ui_egui::Bridge;
use wasm_bindgen::prelude::*;
use web_sys::console;

/// Registers every tool with the browser, if it supports WebMCP.
pub fn register(bridge: &Bridge) {
    let Some(context) = model_context() else {
        console::info_1(&"WebMCP is not available in this browser: agent tools are not registered.".into());
        return;
    };
    let Some(register_tool) =
        Reflect::get(&context, &"registerTool".into()).ok().and_then(|f| f.dyn_into::<Function>().ok())
    else {
        console::warn_1(&"modelContext has no registerTool(): agent tools are not registered.".into());
        return;
    };

    let mut registered = 0;
    for definition in Bridge::definitions() {
        let name = definition["name"].as_str().unwrap_or_default().to_string();
        let Ok(tool) = JSON::parse(&definition.to_string()) else {
            continue;
        };
        let bridge = bridge.clone();
        let tool_name = name.clone();
        let execute =
            Closure::<dyn FnMut(JsValue, JsValue) -> Promise>::new(move |input: JsValue, _client: JsValue| {
                Promise::resolve(&run(&bridge, &tool_name, &input))
            });
        if Reflect::set(&tool, &"execute".into(), execute.as_ref()).is_err() {
            continue;
        }
        // The browser keeps the tool for the life of the page.
        execute.forget();

        match register_tool.call1(&context, &tool) {
            Ok(result) => {
                registered += 1;
                // Newer browsers answer with a promise that rejects if the tool cannot be registered.
                if let Ok(promise) = result.dyn_into::<Promise>() {
                    let failed = Closure::<dyn FnMut(JsValue)>::new(move |e: JsValue| {
                        console::warn_2(&format!("WebMCP: cannot register {name}:").into(), &e);
                    });
                    let _ = promise.catch(&failed);
                    failed.forget();
                }
            }
            Err(e) => console::warn_2(&"WebMCP: registerTool failed:".into(), &e),
        }
    }
    console::info_1(&format!("WebMCP: registered {registered} tools").into());
}

/// `document.modelContext`, or `navigator.modelContext` in earlier previews.
fn model_context() -> Option<JsValue> {
    let global = js_sys::global();
    ["document", "navigator"].into_iter().find_map(|owner| {
        let owner = Reflect::get(&global, &owner.into()).ok().filter(JsValue::is_object)?;
        Reflect::get(&owner, &"modelContext".into()).ok().filter(JsValue::is_object)
    })
}

/// Runs a tool and answers in the shape agents expect: text content, flagged when it failed.
fn run(bridge: &Bridge, name: &str, input: &JsValue) -> JsValue {
    let args = JSON::stringify(input)
        .ok()
        .and_then(|s| s.as_string())
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({}));
    let reply = match bridge.call(name, &args) {
        Ok(value) => json!({ "content": [{ "type": "text", "text": value.to_string() }] }),
        Err(message) => json!({ "content": [{ "type": "text", "text": message }], "isError": true }),
    };
    JSON::parse(&reply.to_string()).unwrap_or(JsValue::NULL)
}

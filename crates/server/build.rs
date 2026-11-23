//! Chooses the web UI the server embeds (src/assets.rs):
//!
//! - `STELLARATOR_WEB_UI=nsh` (the default): the nsh app's web build, `app/.nsh/web/Stellarator`
//!   (`nsh package --target web --out .nsh/web` in app/).
//! - `STELLARATOR_WEB_UI=react`: the React UI, `frontend/dist` (`npm run build` in frontend/). The
//!   Tauri desktop app (desktop/) loads it in its webview; it stays until the switch to nsh is done.
//!
//! Release builds embed the folder; debug builds read it from disk while they run. A missing folder
//! builds a server that answers "the web UI is not built" (the API works without it).

fn main() {
    println!("cargo:rerun-if-env-changed=STELLARATOR_WEB_UI");
    println!("cargo:rustc-check-cfg=cfg(web_ui_react)");
    match std::env::var("STELLARATOR_WEB_UI").as_deref() {
        Ok("react") => println!("cargo:rustc-cfg=web_ui_react"),
        Ok("nsh") | Ok("") | Err(_) => {}
        Ok(other) => panic!("STELLARATOR_WEB_UI is {other:?}: use nsh or react"),
    }
}

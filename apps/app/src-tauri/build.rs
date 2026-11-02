// The scrambling of the built-in Twitch client ID, shared with the app.
include!("src/twitch/scramble.rs");

/// A seed nobody can predict: std's per-process random hasher keys plus the clock.
fn random_seed() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
    h.write_u32(std::process::id());
    h.finish()
}

/// Release builds get the Twitch client ID from the CI secret
/// (`TTV_TWITCH_CLIENT_ID`). It goes into the binary scrambled with a new
/// random seed every build – never as text – and only exists unscrambled in
/// memory while a login needs it.
fn twitch_client_id() {
    println!("cargo:rerun-if-env-changed=TTV_TWITCH_CLIENT_ID");
    let out = std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("twitch_id.rs");
    let code = match std::env::var("TTV_TWITCH_CLIENT_ID").ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()) {
        Some(id) => {
            let seed = random_seed();
            format!("const SCRAMBLED: Option<(&[u8], u64)> = Some((&{:?}, {seed}));\n", scramble(&id, seed))
        }
        None => "const SCRAMBLED: Option<(&[u8], u64)> = None;\n".to_string(),
    };
    std::fs::write(out, code).expect("twitch_id.rs");
}

fn main() {
    twitch_client_id();
    // Test executables link Tauri code that needs Common Controls v6 and fail
    // to start (STATUS_ENTRYPOINT_NOT_FOUND) without the manifest that Tauri
    // only embeds into the app. So the manifest is embedded by the linker for
    // every target of this crate instead (Tauri's documented approach).
    let mut attrs = tauri_build::Attributes::new();
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        attrs = attrs.windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest());
        let manifest = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("windows-app-manifest.xml");
        println!("cargo:rerun-if-changed={}", manifest.display());
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
    }
    tauri_build::try_build(attrs).expect("tauri build");
}

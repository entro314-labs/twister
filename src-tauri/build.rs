fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    // Declaring the commands generates an `allow-<command>` permission for
    // each, which is what lets the capability files hand the x.com webview
    // exactly three of them and the shell all the rest. Without a manifest
    // every app command is open to every local webview, and a remote page can
    // reach none — neither is the shape this app needs.
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            // Shell
            "get_settings",
            "update_settings",
            "get_site_state",
            "navigate_site",
            "site_action",
            "set_site_insets",
            "set_site_visible",
            "sign_out",
            "shell_ready",
            "set_window_material",
            "list_user_assets",
            "open_user_assets_dir",
            "reload_site",
            "show_tooltip",
            "hide_tooltip",
            // Tooltip window
            "tooltip_ready",
            // Bridge, callable from x.com
            "site_settings",
            "site_navigated",
            "site_profile",
        ]),
    ))
    .expect("tauri-build failed");
}

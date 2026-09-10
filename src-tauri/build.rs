fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    // Declaring the commands generates an `allow-<command>` permission for
    // each, which is what lets the capability files hand the x.com webviews
    // exactly the bridge's commands and the shell all the rest. Without a
    // manifest every app command is open to every local webview, and a
    // remote page can reach none — neither is the shape this app needs.
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
            "new_tab",
            "close_tab",
            "activate_tab",
            "get_store_counts",
            "list_people",
            "list_posts",
            "export_people",
            "export_posts",
            "clear_captured",
            "start_op",
            "cancel_op",
            "get_ops",
            "prepare_post",
            "post_now",
            "schedule_post",
            "list_scheduled_posts",
            "delete_scheduled_post",
            "open_downloads_dir",
            // Tooltip window
            "tooltip_ready",
            // Bridge, callable from x.com
            "site_settings",
            "site_navigated",
            "site_profile",
            "site_capture",
            "site_op_progress",
            "site_download",
            "site_layout",
        ]),
    ))
    .expect("tauri-build failed");
}

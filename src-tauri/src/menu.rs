//! The application menu. Its accelerators are how keyboard shortcuts work at
//! all: a key pressed while x.com has focus never reaches the shell webview,
//! but a menu accelerator fires whichever webview is focused. It is also what
//! makes ⌘C and ⌘V work on macOS, where an app with no Edit menu has neither.

use serde::Serialize;
use tauri::menu::{
    AboutMetadataBuilder, Menu, MenuBuilder, MenuEvent, MenuItemBuilder, SubmenuBuilder,
};
use tauri::{AppHandle, Emitter, EventTarget, Runtime};

use crate::site::{self, Action, Destination, SHELL_LABEL};

/// Something the shell has to act on rather than the site: opening its own
/// settings screen, collapsing its sidebar. Payload: [`ShellAction`].
pub const EVENT_SHELL: &str = "twister://shell";

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ShellAction {
    OpenSettings,
    ToggleSidebar,
    OpenPeople,
    OpenPosts,
    OpenCompose,
}

const NAV: &[(&str, &str, &str, Destination)] = &[
    ("nav:home", "Home", "CmdOrCtrl+1", Destination::Home),
    (
        "nav:explore",
        "Explore",
        "CmdOrCtrl+2",
        Destination::Explore,
    ),
    (
        "nav:notifications",
        "Notifications",
        "CmdOrCtrl+3",
        Destination::Notifications,
    ),
    (
        "nav:messages",
        "Messages",
        "CmdOrCtrl+4",
        Destination::Messages,
    ),
    (
        "nav:bookmarks",
        "Bookmarks",
        "CmdOrCtrl+5",
        Destination::Bookmarks,
    ),
    (
        "nav:profile",
        "Profile",
        "CmdOrCtrl+6",
        Destination::Profile,
    ),
];

// One menu, one function: splitting it by submenu would only scatter the
// accelerator table.
#[allow(clippy::too_many_lines)]
pub fn build<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let about = AboutMetadataBuilder::new()
        .name(Some("Twister"))
        .version(Some(env!("CARGO_PKG_VERSION")))
        .authors(Some(vec!["entro314-labs".into()]))
        .comments(Some("A desktop client for X with niceties injected."))
        .build();

    let app_menu = SubmenuBuilder::new(app, "Twister")
        .about(Some(about))
        .separator()
        .item(
            &MenuItemBuilder::with_id("shell:settings", "Settings…")
                .accelerator("CmdOrCtrl+Comma")
                .build(app)?,
        )
        .separator()
        .services()
        .separator()
        .hide()
        .hide_others()
        .show_all()
        .separator()
        .quit()
        .build()?;

    let file = SubmenuBuilder::new(app, "File")
        .item(
            &MenuItemBuilder::with_id("nav:compose", "New Post")
                .accelerator("CmdOrCtrl+N")
                .build(app)?,
        )
        .item(
            &MenuItemBuilder::with_id("tab:new", "New Tab")
                .accelerator("CmdOrCtrl+T")
                .build(app)?,
        )
        .separator()
        .item(
            &MenuItemBuilder::with_id("tab:close", "Close Tab")
                .accelerator("CmdOrCtrl+W")
                .build(app)?,
        )
        .build()?;

    let edit = SubmenuBuilder::new(app, "Edit")
        .undo()
        .redo()
        .separator()
        .cut()
        .copy()
        .paste()
        .select_all()
        .build()?;

    let mut view = SubmenuBuilder::new(app, "View");
    for (id, label, accelerator, _) in NAV {
        view = view.item(
            &MenuItemBuilder::with_id(*id, *label)
                .accelerator(*accelerator)
                .build(app)?,
        );
    }
    let view = view
        .separator()
        .item(
            &MenuItemBuilder::with_id("site:back", "Back")
                .accelerator("CmdOrCtrl+BracketLeft")
                .build(app)?,
        )
        .item(
            &MenuItemBuilder::with_id("site:forward", "Forward")
                .accelerator("CmdOrCtrl+BracketRight")
                .build(app)?,
        )
        .item(
            &MenuItemBuilder::with_id("site:reload", "Reload")
                .accelerator("CmdOrCtrl+R")
                .build(app)?,
        )
        .separator()
        .item(
            &MenuItemBuilder::with_id("shell:sidebar", "Toggle Sidebar")
                .accelerator("CmdOrCtrl+Backslash")
                .build(app)?,
        )
        .separator()
        .item(
            &MenuItemBuilder::with_id("tab:next", "Next Tab")
                .accelerator("Ctrl+Tab")
                .build(app)?,
        )
        .item(
            &MenuItemBuilder::with_id("tab:previous", "Previous Tab")
                .accelerator("Ctrl+Shift+Tab")
                .build(app)?,
        )
        .fullscreen()
        .build()?;

    let tools = SubmenuBuilder::new(app, "Tools")
        .item(
            &MenuItemBuilder::with_id("shell:people", "People")
                .accelerator("CmdOrCtrl+Shift+P")
                .build(app)?,
        )
        .item(
            &MenuItemBuilder::with_id("shell:posts", "Posts")
                .accelerator("CmdOrCtrl+Shift+O")
                .build(app)?,
        )
        .item(
            &MenuItemBuilder::with_id("shell:compose", "Write")
                .accelerator("CmdOrCtrl+Shift+N")
                .build(app)?,
        )
        .build()?;

    let window = SubmenuBuilder::new(app, "Window")
        .minimize()
        .maximize()
        .separator()
        .bring_all_to_front()
        .build()?;

    MenuBuilder::new(app)
        .items(&[&app_menu, &file, &edit, &view, &tools, &window])
        .build()
}

pub fn handle(app: &AppHandle, event: MenuEvent) {
    let id = event.id().as_ref();
    let result = match id {
        "site:back" => site::act(app, Action::Back),
        "site:forward" => site::act(app, Action::Forward),
        "site:reload" => site::act(app, Action::Reload),
        "nav:compose" => site::go(app, Destination::Compose),
        "tab:new" => site::new_tab(app, None).map(|_| ()),
        "tab:close" => match site::state(app).active {
            0 => Ok(()),
            id => site::close_tab(app, id),
        },
        "tab:next" => site::step_tab(app, 1),
        "tab:previous" => site::step_tab(app, -1),
        "shell:settings" => emit_shell(app, ShellAction::OpenSettings),
        "shell:sidebar" => emit_shell(app, ShellAction::ToggleSidebar),
        "shell:people" => emit_shell(app, ShellAction::OpenPeople),
        "shell:posts" => emit_shell(app, ShellAction::OpenPosts),
        "shell:compose" => emit_shell(app, ShellAction::OpenCompose),
        _ => match NAV.iter().find(|(nav_id, ..)| *nav_id == id) {
            Some((_, _, _, destination)) => site::go(app, *destination),
            None => return,
        },
    };
    if let Err(err) = result {
        // A menu item that cannot act says why in the status bar rather than
        // failing silently — `Profile` before the handle is known, for one.
        site::notify(app, err.to_string().replace("[INVALID_INPUT] ", ""));
    }
}

fn emit_shell(app: &AppHandle, action: ShellAction) -> crate::error::Result<()> {
    app.emit_to(EventTarget::webview(SHELL_LABEL), EVENT_SHELL, action)?;
    Ok(())
}

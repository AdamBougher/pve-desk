//! Linux: a native GTK tab strip above two webviews in one window (F14), a
//! native status label (F21), Log out in the top bar (F33), and certificate
//! pinning through WebKitGTK (F4).
//!
//! Moved from `commands.rs` and `lib.rs` by F38 without change.

use super::Platform;
use crate::commands::{end_session, show_admin, ADMIN_WEBVIEW};
use crate::config;
use crate::error::{Error, Result};
use std::path::Path;
use tauri::webview::WebviewBuilder;
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, Url, Webview, WebviewUrl};

/// Label of the window and local webview the frontend runs in.
///
/// Only the Linux build needs it: the tab strip and the child admin webview
/// both address the main window by label. Elsewhere the admin panel is its
/// own window and nothing looks the main one up.
const MAIN: &str = "main";

pub struct Linux;

impl Platform for Linux {
    fn before_start() {
        apply_linux_webkit_workarounds();
    }

    /// F14: on Linux the tab bar is a native widget, because GTK can neither
    /// position a child webview nor cap one's height. See DECISIONS.md.
    /// Nothing to re-fit on resize: the GtkBox lays its children out itself.
    fn install(app: &AppHandle) {
        install_native_tabs(app);
    }

    fn has_top_bar() -> bool {
        true
    }

    /// A second webview in the main window, under the native tab strip.
    fn create_admin(app: &AppHandle, label: &str, blank: Url) -> Result<Webview> {
        let window = app
            .get_window(MAIN)
            .ok_or(Error::Config("main window is gone".into()))?;
        // Position and size are ignored here — the GtkBox decides (see
        // `layout_admin`) — but `add_child` requires them.
        let webview = window.add_child(
            WebviewBuilder::new(label, WebviewUrl::External(blank)),
            LogicalPosition::new(0.0, 0.0),
            LogicalSize::new(800.0, 600.0),
        )?;
        layout_admin(app, true);
        Ok(webview)
    }

    fn show(app: &AppHandle) -> Result<()> {
        layout_admin(app, true);
        set_main_title(app, "Admin");
        // F23: Home's poll pauses while nothing can see it. Non-Linux has no
        // tab concept — Admin is its own window there, Home's window stays
        // fully visible — so this is Linux-only like the title call beside it.
        let _ = app.emit("tab", "admin");
        Ok(())
    }

    fn hide(app: &AppHandle) -> Result<()> {
        if let Some(webview) = app.get_webview(ADMIN_WEBVIEW) {
            webview.hide()?;
        }
        // The local webview expands to fill the window again.
        Self::show_home(app);
        Ok(())
    }

    fn show_home(app: &AppHandle) {
        layout_admin(app, false);
        set_main_title(app, "Home");
    }

    /// Write a status message to the native label beside the tab strip (F21).
    ///
    /// `main.js`'s `status()` calls this in addition to its own page text, so
    /// the message is visible even while Admin covers the page.
    fn set_status(app: &AppHandle, msg: String) {
        let _ = app.run_on_main_thread(move || {
            STATUS_LABEL.with_borrow(|label| {
                if let Some(label) = label {
                    gtk::prelude::LabelExt::set_text(label, &msg);
                }
            });
        });
    }

    /// Grey the Admin tab and Log out until there is a session to act on.
    ///
    /// Without this the buttons are live on the launcher, where pressing them
    /// can only produce an error or do nothing.
    fn set_admin_enabled(app: &AppHandle, enabled: bool) {
        let _ = app.run_on_main_thread(move || {
            SESSION_BUTTONS.with_borrow(|buttons| {
                for button in buttons {
                    gtk::prelude::WidgetExt::set_sensitive(button, enabled);
                }
            });
        });
    }

    fn attach(
        webview: &Webview,
        app: AppHandle,
        entry: String,
        hostname: String,
        target: Url,
    ) -> Result<()> {
        setup_admin_webview(webview, app, entry, hostname, target.to_string())
    }

    /// Linux has a stable absolute path.
    fn default_viewer() -> &'static str {
        "/usr/bin/remote-viewer"
    }

    /// `mode` applies only when the file is created, so `create_new` is used
    /// rather than `create`: it fails instead of writing a ticket into a path
    /// something else already owns, including a symlink planted in a shared
    /// temp directory.
    fn create_private(path: &Path) -> std::io::Result<std::fs::File> {
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
    }

    fn restrict_dir(dir: &Path) -> std::io::Result<()> {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
    }

    fn tray_exists(app: &AppHandle) -> bool {
        crate::tray::exists(app)
    }
}

/// Pick a WebKitGTK rendering path that works on this machine.
///
/// WebKit's dmabuf renderer allocates GBM buffers, which fails on the NVIDIA
/// proprietary driver: on a native Wayland session GDK then dies with
/// "Error 71 (Protocol error)" before any window appears.
///
/// Default is `WEBKIT_DMABUF_RENDERER_FORCE_SHM=1`: the dmabuf renderer and
/// accelerated compositing stay on, but buffers go through shared memory
/// instead of GBM. Verified on this machine to start cleanly on Wayland with
/// no errors and render correctly.
///
/// `PVE_DESK_RENDER` overrides it while the console-smoothness problem is
/// being chased:
///   - `legacy` — disable the dmabuf renderer entirely (the previous default)
///   - `raw`    — set nothing; WebKit's stock behaviour, crashes on NVIDIA
fn apply_linux_webkit_workarounds() {
    match std::env::var("PVE_DESK_RENDER").as_deref() {
        Ok("raw") => {}
        Ok("legacy") => {
            if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
                std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
            }
        }
        _ => {
            if std::env::var_os("WEBKIT_DMABUF_RENDERER_FORCE_SHM").is_none() {
                std::env::set_var("WEBKIT_DMABUF_RENDERER_FORCE_SHM", "1");
            }
        }
    }
}

/// Fit the two webviews around the tab bar.
///
/// **Linux has no webview geometry, and no way to cap a widget's height.**
/// `tauri-runtime-wry` builds a child webview into the window's `GtkBox`, and
/// wry packs it with `pack_start(webview, true, true, 0)` with
/// `is_in_fixed_parent = false`, so `set_bounds` is ignored. Nor can the
/// local webview simply be pinned to the tab bar's height: `set_size_request`
/// sets a *minimum*, and GTK3 hands a box child its **natural** size before
/// giving anything to expanding siblings. Measured: with `req = -1x36` the
/// local webview reported `preferred_h = (min 36, nat 700)` and took all 700
/// pixels, leaving the admin webview 1.
///
/// So the tab bar is a native GTK widget (see `install_native_tabs`) and
/// exactly one webview is visible at a time — the visible one fills the space
/// below the strip, which needs no capping at all.
fn layout_admin(app: &AppHandle, admin_visible: bool) {
    let Some(local) = app.get_webview(MAIN) else {
        return;
    };
    let _ = if admin_visible {
        local.hide()
    } else {
        local.show()
    };
}

/// Window title follows the visible tab (F21, Q21.1). Linux-only, like the
/// tabs themselves: elsewhere Admin is its own window with its own title
/// ("Proxmox"), and the main window never shows anything else.
fn set_main_title(app: &AppHandle, tab: &str) {
    // Reads tauri.conf.json's own "version" field via Tauri's PackageInfo,
    // not a second hardcoded copy — the same field the release workflow's
    // version guard already cross-checks against the tag and the PKGBUILD.
    let version = &app.package_info().version;
    if let Some(window) = app.get_window(MAIN) {
        let _ = window.set_title(&format!("pve-desk {version} — {tab}"));
    }
}

// The Admin and Log out buttons, so their sensitivity can be toggled with the
// session (F33 Q33.3: one switch greys both).
// A thread-local rather than app state because GTK widgets are not `Send`:
// everything that touches it runs on the main thread, either inside
// `with_webview` or via `run_on_main_thread`.
thread_local! {
    static SESSION_BUTTONS: std::cell::RefCell<Vec<gtk::Button>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

// The native status label beside the tab strip (F21, Q21.2).
// Exists so a certificate message from `app.emit("admin-cert", ...)` is
// never hidden: it fires while the admin webview is loading, which is
// exactly when the page's own `#statusbar` is covered. Same thread-local
// pattern as `SESSION_BUTTONS` and the same reason — GTK widgets are not `Send`.
thread_local! {
    static STATUS_LABEL: std::cell::RefCell<Option<gtk::Label>> =
        const { std::cell::RefCell::new(None) };
}

/// Style for the tab strip. Uses the theme's own colours rather than fixed
/// ones, so it follows whatever GTK theme is in use.
const TAB_CSS: &str = "
.pve-tabstrip {
  padding: 4px 6px;
  border-bottom: 1px solid @borders;
}
.pve-tabstrip button {
  padding: 1px 18px;
  min-height: 26px;
}
";

/// The tab bar, as real GTK buttons above both webviews.
///
/// Built once at startup. Switching is driven from here rather than from the
/// page: while the admin panel is up the local webview is hidden, and relying
/// on a hidden webview's JavaScript to be able to switch back would be a way
/// to get stuck.
fn install_native_tabs(app: &AppHandle) {
    use gtk::prelude::{
        BoxExt, ButtonExt, Cast, CssProviderExt, LabelExt, StyleContextExt, WidgetExt,
    };

    let Some(local) = app.get_webview(MAIN) else {
        return;
    };
    let app = app.clone();
    let _ = local.with_webview(move |platform| {
        let Some(parent) = platform.inner().parent() else {
            return;
        };
        let Ok(vbox) = parent.downcast::<gtk::Box>() else {
            return;
        };

        let strip = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        strip.style_context().add_class("pve-tabstrip");

        // "linked" is GTK's own segmented-control look, so the pair reads as
        // one control in whatever theme is running.
        let group = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        group.style_context().add_class("linked");

        let admin = gtk::Button::with_label("Admin");
        let home = gtk::Button::with_label("Home");
        group.pack_start(&admin, false, false, 0);
        group.pack_start(&home, false, false, 0);

        // F33 Q33.1: the bar reads `[status text …] [Admin | Home]  [Log out]`.
        // Log out is outside the linked group, with a gap after Home, so it
        // does not read as a third tab and a near-miss on Home does not log
        // out. `pack_end` fills from the right, so Log out goes in first.
        let logout = gtk::Button::with_label("Log out");
        logout.set_margin_start(12);
        strip.pack_end(&logout, false, false, 0);
        strip.pack_end(&group, false, false, 0);

        // F21 Q21.2: a native label for status text, so an admin-cert
        // message (in particular the REFUSED one — CLAUDE.md's hard rule on
        // an unknown certificate) is visible even while Admin covers the
        // page's own #statusbar. Takes the remaining width on the left, and
        // reads from the left edge rather than centred in that space.
        let status_label = gtk::Label::new(None);
        status_label.set_xalign(0.0);
        strip.pack_start(&status_label, true, true, 6);
        STATUS_LABEL.with_borrow_mut(|label| *label = Some(status_label));

        // Nothing to act on until a session exists; `set_admin_enabled`
        // turns both on at connect and off again at logout or session loss.
        admin.set_sensitive(false);
        logout.set_sensitive(false);
        SESSION_BUTTONS.with_borrow_mut(|buttons| {
            *buttons = vec![admin.clone(), logout.clone()];
        });

        if let Some(screen) = gtk::gdk::Screen::default() {
            let provider = gtk::CssProvider::new();
            if provider.load_from_data(TAB_CSS.as_bytes()).is_ok() {
                gtk::StyleContext::add_provider_for_screen(
                    &screen,
                    &provider,
                    gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
                );
            }
        }

        let app_admin = app.clone();
        admin.connect_clicked(move |_| {
            let app = app_admin.clone();
            // Opening may need a login, so it cannot run on this thread.
            tauri::async_runtime::spawn(async move {
                if let Err(e) = show_admin(&app).await {
                    let _ = app.emit("admin-cert", format!("admin panel: {e}"));
                }
            });
        });

        let app_home = app.clone();
        home.connect_clicked(move |_| {
            if let Some(webview) = app_home.get_webview(ADMIN_WEBVIEW) {
                let _ = webview.hide();
            }
            layout_admin(&app_home, false);
            set_main_title(&app_home, "Home");
            let _ = app_home.emit("tab", "home");
        });

        // Q33.2: the backend ends the session itself, so the user lands on
        // Home with no session even if the hidden page's JavaScript never
        // answers.
        let app_logout = app.clone();
        logout.connect_clicked(move |_| {
            if let Err(e) = end_session(&app_logout) {
                Linux::set_status(&app_logout, format!("Log out failed: {e}"));
            }
        });

        // Above both webviews. A non-expanding button box has a small,
        // truthful natural height, which is exactly what a webview does not.
        vbox.pack_start(&strip, false, false, 0);
        vbox.reorder_child(&strip, 0);
        strip.show_all();

        // F21 Q21.1: the app always opens on Home (F16), so the title must
        // say so from the first frame, not only after the first tab switch.
        // Caught by actually running the app — app-shot.sh showed the tab
        // strip correctly reading "Home" while the titlebar still said the
        // bare "pve-desk" from tauri.conf.json, because every other call to
        // set_main_title sits at a transition point that hadn't fired yet.
        set_main_title(&app, "Home");
    });
}

/// Configure the admin panel's webview, then load it.
///
/// The Proxmox cert is signed by the cluster's own CA, which is not in the
/// system trust store WebKitGTK reads, so the load fails with a TLS error and
/// WebKitGTK offers no way to click through. We catch that failure, check the
/// certificate's SHA-256 against the fingerprint saved in the config, allow
/// that exact certificate for that one host, and retry the load.
///
/// First connect stores the fingerprint it sees (and reports it to the UI).
/// Afterwards a certificate that does not match is refused — the window keeps
/// WebKit's error page rather than trusting a cert that changed underneath us.
///
/// Narrower than trusting the cluster CA system-wide: that would let the CA
/// vouch for any host, this accepts one certificate for one host, inside this
/// app only.
fn setup_admin_webview(
    webview: &Webview,
    app: AppHandle,
    entry: String,
    hostname: String,
    target: String,
) -> Result<()> {
    use std::cell::Cell;
    use webkit2gtk::gio::prelude::TlsCertificateExt;
    use webkit2gtk::glib;
    use webkit2gtk::{WebContextExt, WebViewExt};

    webview.with_webview(move |platform| {
        let webview = platform.inner();
        // One retry only: if the load still fails after allowing the cert,
        // let the error page stand instead of looping.
        let retried = Cell::new(false);

        webview.connect_load_failed_with_tls_errors(move |webview, uri, cert, _errors| {
            if retried.replace(true) {
                return false;
            }
            let Some(der) = cert.certificate() else {
                return false;
            };
            let Some(seen) = glib::compute_checksum_for_data(glib::ChecksumType::Sha256, &der)
            else {
                return false;
            };
            let seen = seen.to_string();

            match pin_fingerprint(&entry, &seen) {
                Ok(Pinned::Matches) => {}
                Ok(Pinned::Stored) => {
                    let _ = app.emit("admin-cert", format!("pinned node certificate {seen}"));
                }
                Ok(Pinned::Mismatch(expected)) => {
                    let _ = app.emit(
                        "admin-cert",
                        format!(
                            "REFUSED: node certificate is {seen}, expected {expected}. \
                             Not loading the admin panel."
                        ),
                    );
                    return false;
                }
                Err(e) => {
                    let _ = app.emit("admin-cert", format!("certificate check failed: {e}"));
                    return false;
                }
            }

            let Some(context) = webview.context() else {
                return false;
            };
            context.allow_tls_certificate_for_host(cert, &hostname);
            webview.load_uri(uri);
            true
        });

        webview.load_uri(&target);
    })?;
    Ok(())
}

/// Outcome of comparing a certificate against the pinned fingerprint.
enum Pinned {
    /// Matches what the config already pinned.
    Matches,
    /// Nothing was pinned; this fingerprint has now been saved.
    Stored,
    /// Does not match the pinned fingerprint, which is carried back.
    Mismatch(String),
}

/// Compare `seen` against the active host's pinned fingerprint, storing it
/// if nothing is pinned yet.
fn pin_fingerprint(entry: &str, seen: &str) -> Result<Pinned> {
    let mut cfg = config::load()?.ok_or(Error::NotConfigured)?;
    let pinned = cfg.host(entry)?.cert_fingerprint.clone();
    match pinned {
        Some(expected) if expected == seen => Ok(Pinned::Matches),
        Some(expected) => Ok(Pinned::Mismatch(expected)),
        None => {
            let target = cfg
                .hosts
                .iter_mut()
                .find(|h| h.name == entry)
                .ok_or_else(|| Error::Config(format!("no connection named {entry:?}")))?;
            target.cert_fingerprint = Some(seen.to_string());
            config::save(&cfg)?;
            Ok(Pinned::Stored)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Linux, Platform};

    /// The point of the `.vv` handling: the file holding the ticket is never
    /// readable by anyone else, including for the instant between creating it
    /// and writing to it.
    #[test]
    fn vv_file_is_created_private() {
        use std::os::unix::fs::PermissionsExt;

        let dir = std::env::temp_dir().join(format!("pve-desk-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("mode-check.vv");
        let _ = std::fs::remove_file(&path);

        let f = Linux::create_private(&path).unwrap();
        let mode = f.metadata().unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "created as {mode:o}, want 600");

        // create_new: a path that already exists is refused rather than
        // written through, which is what stops a planted symlink.
        assert!(Linux::create_private(&path).is_err());

        std::fs::remove_dir_all(&dir).unwrap();
    }
}

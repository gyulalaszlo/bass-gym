//! The macOS menu bar: the application menu (About, Preferences… ⌘,, Services, Hide,
//! Quit) plus Edit and Window menus. Elsewhere there is no menu bar; preferences open
//! from the sidebar or with Ctrl+,.

/// What a menu item asks the app to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuAction {
    Preferences,
    Quit,
}

#[cfg(target_os = "macos")]
mod imp {
    use muda::accelerator::{Accelerator, Code, Modifiers};
    use muda::{AboutMetadata, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};

    use super::MenuAction;

    const PREFS: &str = "preferences";
    const QUIT: &str = "quit";

    /// Keeps the menu alive.
    pub struct AppMenu {
        _menu: Menu,
    }

    impl AppMenu {
        pub fn install() -> Option<Self> {
            match build() {
                Ok(menu) => Some(Self { _menu: menu }),
                Err(e) => {
                    log::warn!("couldn't set up the menu bar: {e}");
                    None
                }
            }
        }

        pub fn poll(&self) -> Option<MenuAction> {
            while let Ok(ev) = MenuEvent::receiver().try_recv() {
                match ev.id.as_ref() {
                    PREFS => return Some(MenuAction::Preferences),
                    QUIT => return Some(MenuAction::Quit),
                    _ => {}
                }
            }
            None
        }
    }

    fn build() -> muda::Result<Menu> {
        let sep = PredefinedMenuItem::separator;
        let about = AboutMetadata {
            name: Some("bass-gym".into()),
            version: Some(env!("CARGO_PKG_VERSION").into()),
            ..Default::default()
        };
        let prefs = MenuItem::with_id(
            PREFS,
            "Preferences…",
            true,
            Some(Accelerator::new(Some(Modifiers::SUPER), Code::Comma)),
        );
        // Quit through the app (not NSApp's terminate:) so settings and state are saved.
        let quit = MenuItem::with_id(
            QUIT,
            "Quit bass-gym",
            true,
            Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyQ)),
        );
        let app = Submenu::with_items(
            "bass-gym",
            true,
            &[
                &PredefinedMenuItem::about(None, Some(about)),
                &sep(),
                &prefs,
                &sep(),
                &PredefinedMenuItem::services(None),
                &sep(),
                &PredefinedMenuItem::hide(None),
                &PredefinedMenuItem::hide_others(None),
                &PredefinedMenuItem::show_all(None),
                &sep(),
                &quit,
            ],
        )?;
        let edit = Submenu::with_items(
            "Edit",
            true,
            &[
                &PredefinedMenuItem::undo(None),
                &PredefinedMenuItem::redo(None),
                &sep(),
                &PredefinedMenuItem::cut(None),
                &PredefinedMenuItem::copy(None),
                &PredefinedMenuItem::paste(None),
                &PredefinedMenuItem::select_all(None),
            ],
        )?;
        let window = Submenu::with_items(
            "Window",
            true,
            &[
                &PredefinedMenuItem::minimize(None),
                &PredefinedMenuItem::maximize(None),
                &PredefinedMenuItem::fullscreen(None),
                &sep(),
                &PredefinedMenuItem::close_window(None),
                &sep(),
                &PredefinedMenuItem::bring_all_to_front(None),
            ],
        )?;
        let menu = Menu::with_items(&[&app, &edit, &window])?;
        menu.init_for_nsapp();
        window.set_as_windows_menu_for_nsapp();
        Ok(menu)
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use super::MenuAction;

    pub struct AppMenu;

    impl AppMenu {
        pub fn install() -> Option<Self> {
            None
        }

        pub fn poll(&self) -> Option<MenuAction> {
            None
        }
    }
}

pub use imp::AppMenu;

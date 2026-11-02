//! Live Settings: reads `settings.json` when the app starts, applies the theme, and saves every
//! change at once.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use slint::ComponentHandle;
use tidewise_core::settings::{self, Settings};

use crate::{settings_view, MainWindow, SettingsState, Theme};

pub struct Ctl {
    settings: Settings,
    dir: Option<PathBuf>,
    problem: Option<String>,
}

impl Ctl {
    /// Read the user's settings (defaults when there are none or they are broken).
    pub fn load() -> Self {
        let dir = settings::settings_dir();
        let loaded = dir.as_deref().map(settings::load);
        Ctl {
            settings: loaded
                .as_ref()
                .map(|l| l.settings.clone())
                .unwrap_or_default(),
            problem: loaded.and_then(|l| l.problem),
            dir,
        }
    }

    /// Show the settings on the screen and set the theme.
    pub fn apply(&self, ui: &MainWindow) {
        let index = settings_view::theme_index(self.settings.theme);
        ui.global::<Theme>().set_mode(index);
        let st = ui.global::<SettingsState>();
        st.set_theme(index);
        st.set_update_check(self.settings.update_check);
        st.set_update_note(settings_view::update_note(self.settings.update_check).into());
        st.set_about_title(settings_view::about_title(env!("CARGO_PKG_VERSION")).into());
        st.set_about_text(
            "A PC manager for Linux: it cleans junk, shows what uses space, removes apps and watches performance. Free software under the GPL-3.0-or-later. It never runs as root."
                .into(),
        );
        st.set_problem(self.problem.clone().unwrap_or_default().into());
    }

    fn save(&mut self, ui: &MainWindow) {
        self.problem = match &self.dir {
            Some(dir) => settings::save(dir, &self.settings).err().map(|e| {
                format!(
                    "Your change could not be saved ({e}). It applies until you close Tidewise."
                )
            }),
            None => Some(
                "Tidewise could not find your config folder, so the change is not saved.".into(),
            ),
        };
        self.apply(ui);
    }
}

pub fn wire(ui: &MainWindow, ctl: &Rc<RefCell<Ctl>>) {
    let st = ui.global::<SettingsState>();
    st.on_set_theme({
        let (weak, ctl) = (ui.as_weak(), ctl.clone());
        move |i| {
            let (Some(ui), mut c) = (weak.upgrade(), ctl.borrow_mut()) else {
                return;
            };
            c.settings.theme = settings_view::theme_from_index(i);
            c.save(&ui);
        }
    });
    st.on_toggle_update({
        let (weak, ctl) = (ui.as_weak(), ctl.clone());
        move || {
            let (Some(ui), mut c) = (weak.upgrade(), ctl.borrow_mut()) else {
                return;
            };
            c.settings.update_check = !c.settings.update_check;
            c.save(&ui);
        }
    });
}

use std::cell::Cell;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

#[cfg(target_os = "macos")]
use anyhow::Context;
use zeroize::Zeroizing;

use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use mac_ui::tray;
use mac_ui::tray_icon::{
    MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
    menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu},
};
use mac_ui::winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
};

use crate::appearance;
use crate::clipboard::{self, ClipboardHistory, ClipboardView, SecretBytes};
use crate::commands::{self, CardView, CommandId, Hist, LaunchData, SubjectKind};
use crate::format;
use crate::icon;
use crate::image_edit::{ImageOp, ImageVersions};
use crate::launcher::{self, UserEvent};
use crate::table::{TableOp, TableVersions};

/// How often the pasteboard is polled while the card is open.
const REFRESH: Duration = Duration::from_millis(400);
/// One status-item click can be delivered twice (the button action and the tray mouse-up).
/// A second toggle inside this window would open the card and close it again.
const ICON_CLICK_GAP: Duration = Duration::from_millis(50);
/// How often it is polled while the card is closed. A quiet tick reads only the change count.
/// A closed card can wait longer: the change count is cheap to read, and a new copy is
/// still noticed within a few seconds. winit's `WaitUntil` has no timer tolerance like an
/// `NSTimer` would (`setTolerance`); the wake is exact.
const IDLE_REFRESH: Duration = Duration::from_secs(3);
/// A copy copycraft wrote that is labelled sensitive is cleared from the pasteboard this long
/// after the write, when that setting is on and nothing else was copied since.
const SENSITIVE_CLEAR_AFTER: Duration = Duration::from_secs(60);
/// Background work that finishes sooner shows no spinner, so small files do not flicker.
const SPINNER_DELAY: Duration = Duration::from_millis(180);
/// Copies at least this long are classified on a background thread right after the copy, so the
/// card opens without scanning them first.
const PREWARM_LEN: usize = 64 * 1024;

mod ai;
mod app;
mod dispatch;
mod handler;
mod history;
mod image;
mod menu;
mod popup;
mod run;
mod save;
mod source;
mod state;
mod table;
mod text;
mod work;

pub(crate) use work::WorkDone;

use ai::*;
use app::*;
use menu::*;
pub use run::*;
use state::*;
use text::*;

#[cfg(test)]
mod tests;

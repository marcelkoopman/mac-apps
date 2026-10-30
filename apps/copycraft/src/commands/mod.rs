use zeroize::{Zeroize, Zeroizing};

use crate::appearance::Theme;
use crate::clipboard;
use crate::convert;
use crate::dataframe;
use crate::decode;
use crate::format::{self, FormatKind};
use crate::toolbar_visibility;
use mac_ui::keys::Key;

mod actions;
mod ai;
mod card;
mod chips;
mod file_opening;
mod history;
mod kind_filter;
mod layout;
mod menu;
mod menu_rules;
mod meta;
mod panel;
mod strip;
mod table;
mod table_search;
mod table_strip;
mod table_switch;
mod text;
mod types;

pub use actions::*;
pub use ai::*;
pub use card::*;
pub use chips::*;
pub use file_opening::*;
pub use history::*;
pub use kind_filter::*;
pub use layout::*;
pub use menu::*;
pub use menu_rules::*;
pub use meta::*;
pub use panel::*;
pub use strip::*;
pub use table::*;
pub use table_search::*;
pub use table_strip::*;
pub use table_switch::*;
pub use text::*;
pub use types::*;

#[cfg(test)]
pub(crate) mod tests;

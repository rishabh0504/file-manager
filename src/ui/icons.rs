//! Icon names used across the GUI. Glyphs come from the bundled Phosphor icon font
//! (`egui-phosphor`, MIT), so every OS draws the same shape. Registered in `style::configure_app_style`.
//! Use an icon with a tooltip when the button has no text; keep a text label on primary actions.

use egui_phosphor::regular as ph;

#[allow(dead_code)]
pub struct Icons;

#[allow(dead_code)]
impl Icons {
    // Navigation and actions
    pub const NAV_BACK: &'static str = ph::ARROW_LEFT;
    pub const NAV_FWD: &'static str = ph::ARROW_RIGHT;
    pub const NAV_UP: &'static str = ph::ARROW_UP;
    pub const NAV_DOWN: &'static str = ph::ARROW_DOWN;
    pub const REFRESH: &'static str = ph::ARROW_CLOCKWISE;
    pub const CLOSE: &'static str = ph::X;
    pub const SEARCH: &'static str = ph::MAGNIFYING_GLASS;
    pub const NEW_FOLDER: &'static str = ph::FOLDER_PLUS;
    pub const SORT_FILTER: &'static str = ph::FUNNEL;
    pub const MORE: &'static str = ph::DOTS_THREE;
    pub const ZOOM: &'static str = ph::MAGNIFYING_GLASS_PLUS;
    pub const MOVE: &'static str = ph::ARROW_RIGHT;
    pub const COMPARE: &'static str = ph::ARROWS_LEFT_RIGHT;

    // Sorting and views
    pub const SORT_ASC: &'static str = ph::CARET_UP;
    pub const SORT_DESC: &'static str = ph::CARET_DOWN;
    pub const VIEW_LIST: &'static str = ph::LIST;
    pub const VIEW_GRID: &'static str = ph::SQUARES_FOUR;
    pub const VIEW_FOLDERS: &'static str = ph::FOLDERS;

    // Media and playback
    pub const PLAY: &'static str = ph::PLAY;
    pub const PAUSE: &'static str = ph::PAUSE;
    pub const STOP: &'static str = ph::STOP;
    pub const PREV: &'static str = ph::SKIP_BACK;
    pub const NEXT: &'static str = ph::SKIP_FORWARD;
    pub const FULLSCREEN: &'static str = ph::ARROWS_OUT_SIMPLE;
    pub const RESTORE: &'static str = ph::ARROWS_IN_SIMPLE;

    // File types and places
    pub const FOLDER: &'static str = ph::FOLDER;
    pub const FOLDER_OPEN: &'static str = ph::FOLDER_OPEN;
    pub const HOME: &'static str = ph::HOUSE;
    pub const DESKTOP: &'static str = ph::DESKTOP;
    pub const DOCUMENT: &'static str = ph::FILE;
    pub const DOWNLOAD: &'static str = ph::DOWNLOAD_SIMPLE;
    pub const PICTURE: &'static str = ph::IMAGE;
    pub const VIDEO: &'static str = ph::FILM_STRIP;
    pub const AUDIO: &'static str = ph::MUSIC_NOTE;
    pub const ARCHIVE: &'static str = ph::ARCHIVE;

    // Management and queues
    pub const TRASH: &'static str = ph::TRASH;
    pub const INBOX: &'static str = ph::TRAY;
    pub const STAGE: &'static str = ph::TRAY;
    pub const QUEUE: &'static str = ph::LIST_NUMBERS;
    pub const PACKAGE: &'static str = ph::PACKAGE;
    /// Shown in light theme: click for dark.
    pub const THEME_TO_DARK: &'static str = ph::MOON;
    /// Shown in dark theme: click for light.
    pub const THEME_TO_LIGHT: &'static str = ph::SUN;
}

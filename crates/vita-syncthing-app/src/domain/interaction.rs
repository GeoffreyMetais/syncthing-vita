//! Interactive elements of the frame on screen (`HitMap`).
//!
//! A screen declares each interactive element once, while drawing it: where it is, what it
//! does (`UiAction`), which D-pad focus position it holds and which face button triggers it
//! directly. Touch, `Cross` on the focused element and face-button shortcuts are all resolved
//! from that single declaration, so input can never drift from what is displayed (button
//! positions depend on the label widths of the current language).

use crate::domain::models::ActiveTab;

/// Screen rectangle in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }

    pub fn contains(&self, px: i32, py: i32) -> bool {
        px >= self.x && px < self.x + self.w && py >= self.y && py < self.y + self.h
    }
}

/// What activating an interactive element does. Executed by the screen's controller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiAction {
    // --- Any page ---
    /// Activate a sidebar tab.
    SwitchTab(ActiveTab),
    /// Start a sync (or rescan over the running sessions).
    StartSync,
    /// Show this device's ID and QR code.
    ShowDeviceId,
    /// Open the add-device-by-ID dialog.
    AddDevice,
    /// Ask for confirmation before quitting.
    ConfirmQuit,

    // --- Modal dialogs ---
    /// Same as pressing this button (buttons and hints of the open modal).
    Press(PadKey),
    /// Activate row / key `usize` of the open modal's list or keypad.
    ModalRow(usize),
    /// Open the system keyboard (text field of the open modal).
    OpenKeyboard,
    /// Absorbs the touch: nothing happens (e.g. the body of a modal).
    Inert,

    // --- `Overview` share list ---
    /// Open the share creation dialog.
    NewShare,
    /// Select the share row.
    SelectShare(usize),
    /// Open the device linking dialog of the share.
    ManageDevices(usize),
    /// Ask for confirmation before deleting the share.
    DeleteShare(usize),
    /// Browse for another target folder of the share.
    ChangeShareFolder(usize),

    // --- `Files` ---
    /// `Files`: go to the parent folder (or leave the browser at the root).
    PickerUp,
    /// `Files`: choose the folder of the row (share it / use it as target).
    PickerSelect(usize),
    /// `Files`: enter the folder of the row.
    PickerOpen(usize),

    // --- `Settings` ---
    /// `Settings`: switch between the Dark and OLED themes.
    ToggleTheme,
    /// `Settings`: cycle the UI language.
    CycleLanguage,
    /// `Settings`: forget every declined folder offer.
    ClearDeclinedFolders,
    /// `Settings`: open the full-screen protocol log viewer.
    OpenLogs,

    // --- Log viewer ---
    /// Close the log viewer (back to `Settings`).
    CloseLogs,
    /// Switch between this session's lines and the whole `syncthing.log` history.
    ToggleLogHistory,
    /// Scroll one page towards older lines.
    ScrollLogsOlder,
    /// Scroll one page towards newer lines.
    ScrollLogsNewer,
}

/// D-pad focus position an element holds on its page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusSlot {
    /// Button `col` (left to right) of the page's top element (`Files`: the single `Up`
    /// button, `Overview`: the device card buttons).
    Header { col: usize },
    /// Button `col` (left to right) of list row `row`.
    RowAction { row: usize, col: usize },
}

impl FocusSlot {
    /// List row the slot belongs to (`None` for the page header).
    pub fn row(&self) -> Option<usize> {
        match self {
            FocusSlot::Header { .. } => None,
            FocusSlot::RowAction { row, .. } => Some(*row),
        }
    }
}

/// Face button that triggers an element directly, whatever the D-pad focus.
/// (`Cross` is not one: it always activates the focused element.)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutKey {
    Circle,
    Square,
    Triangle,
}

/// A physical button a touch element can stand for (`UiAction::Press`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PadKey {
    Cross,
    Circle,
    Square,
    Triangle,
    L,
    R,
    Start,
    Select,
}

/// One interactive element of the frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hotspot {
    pub rect: Rect,
    pub action: UiAction,
    pub focus: Option<FocusSlot>,
    pub shortcut: Option<ShortcutKey>,
}

/// Interactive elements of the last rendered frame, in drawing order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HitMap {
    hotspots: Vec<Hotspot>,
}

impl HitMap {
    /// Forgets every element (start of a new frame).
    pub fn clear(&mut self) {
        self.hotspots.clear();
    }

    /// Declares an interactive element.
    pub fn add(&mut self, hotspot: Hotspot) {
        self.hotspots.push(hotspot);
    }

    pub fn hotspots(&self) -> &[Hotspot] {
        &self.hotspots
    }

    /// Action of the topmost element under a touch at (`x`, `y`).
    pub fn action_at(&self, x: i32, y: i32) -> Option<UiAction> {
        self.hotspots
            .iter()
            .rev()
            .find(|h| h.rect.contains(x, y))
            .map(|h| h.action)
    }

    /// Action of the element holding D-pad focus position `slot`.
    pub fn focused_action(&self, slot: FocusSlot) -> Option<UiAction> {
        self.hotspots
            .iter()
            .find(|h| h.focus == Some(slot))
            .map(|h| h.action)
    }

    /// Action triggered by face button `key`: an element of the selected list row
    /// `selected_row` first (it is the one the user is looking at), else a page-level element
    /// (page header, hint bar). Elements of the other rows never answer.
    pub fn shortcut_action(
        &self,
        key: ShortcutKey,
        selected_row: Option<usize>,
    ) -> Option<UiAction> {
        let with_key = || {
            self.hotspots
                .iter()
                .filter(move |h| h.shortcut == Some(key))
        };
        let row_of = |h: &Hotspot| h.focus.and_then(|f| f.row());
        selected_row
            .and_then(|selected| with_key().find(|h| row_of(h) == Some(selected)))
            .or_else(|| with_key().find(|h| row_of(h).is_none()))
            .map(|h| h.action)
    }
}

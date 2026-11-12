//! Interactive elements of the frame on screen (`HitMap`).
//!
//! A screen declares each interactive element once, while drawing it: where it is, what it
//! does (`UiAction`), which D-pad focus position it holds and which face button triggers it
//! directly. Touch, `Cross` on the focused element and face-button shortcuts are all resolved
//! from that single declaration, so input can never drift from what is displayed (button
//! positions depend on the label widths of the current language).

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
    /// `Shares`: open the share creation dialog.
    NewShare,
    /// `Shares`: select the share row.
    SelectShare(usize),
    /// `Shares`: open the device linking dialog of the share.
    ManageDevices(usize),
    /// `Shares`: ask for confirmation before deleting the share.
    DeleteShare(usize),
    /// `Shares`: browse for another target folder of the share.
    ChangeShareFolder(usize),
}

/// D-pad focus position an element holds on its page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusSlot {
    /// The page's top button.
    Header,
    /// Button `col` (left to right) of list row `row`.
    RowAction { row: usize, col: usize },
}

impl FocusSlot {
    /// List row the slot belongs to (`None` for the page header).
    pub fn row(&self) -> Option<usize> {
        match self {
            FocusSlot::Header => None,
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

    /// Action triggered by face button `key`: a page-level element, or an element of the
    /// selected list row `selected_row`.
    pub fn shortcut_action(&self, key: ShortcutKey, selected_row: usize) -> Option<UiAction> {
        self.hotspots
            .iter()
            .find(|h| {
                h.shortcut == Some(key)
                    && h.focus
                        .and_then(|f| f.row())
                        .is_none_or(|row| row == selected_row)
            })
            .map(|h| h.action)
    }
}

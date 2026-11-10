//! The join between [`solidcraft_nav_extras`] — the UI-free chord ladder behind the CATIA mouse
//! preset — and everything here that owns a camera: the preferences' preset table, the navigation
//! state machine, the viewport's right-click guard, the preferences legend and the scenario
//! driver's synthetic chord events.
//!
//! The ladder itself lives in `solidcraft-nav-extras`, decided by booleans and tested without a
//! window. What cannot leave this crate is anything that touches `SolveApp`, which is why the
//! two meet in this one file. The preset table in `prefs` carries a [`NavChord`] on the presets
//! that have one, and every caller goes through here — see
//! `docs/design/nav-extras-integration.md` for the hook points and how to keep them across
//! upstream merges.

use egui::{Pos2, Rect};
use solidcore_record::Vec3;
use solidcraft_nav_extras::{ChordSpec, Gear, SideButton, CLICK_WOBBLE};

use crate::SolveApp;
use crate::prefs::{NavButton, nav_mode, nav_preset, pivot_under};
use crate::viewport::{NavMode, projection};

/// A two-button navigation, as CATIA's: while the anchor is held, a side button changes the mode.
/// Pressed and held it turns the view (`held`); pressed and let go with the anchor still down it
/// zooms (`tapped`), and pressing it again turns the view again — the drag changes gear without
/// being let go of. The anchor on its own does what its binding says (pan, in CATIA). The ladder
/// that decides which of the three applies lives in `solidcraft-nav-extras` as [`ChordSpec`];
/// the gears are this crate's, because they name its camera modes.
#[derive(Clone, Copy, Debug)]
pub struct NavChord {
    /// The chord's shape: which button anchors the drag, which sides change its gear, and
    /// whether a bare click of the anchor recentres the view.
    pub spec: ChordSpec,
    /// Side button held down: the mode of the drag until it is let go (CATIA: orbit).
    pub held: NavMode,
    /// Side button let go, anchor still down: the mode of the drag until it is pressed again
    /// (CATIA: zoom).
    pub tapped: NavMode,
}

impl NavChord {
    /// What the drag does this frame, given the side buttons' state, whether one has been pressed
    /// since the anchor went down (the latch, which survives letting go), and the anchor's own
    /// binding. The decision is the ladder's; this only names the gears.
    pub fn mode(&self, side_down: bool, latched: bool, anchor: Option<NavMode>) -> Option<NavMode> {
        match self.spec.gear(side_down, latched) {
            Gear::Anchor => anchor,
            Gear::Held => Some(self.held),
            Gear::Tapped => Some(self.tapped),
        }
    }

    /// Whether `button` is one of the side buttons.
    pub fn is_side(&self, button: NavButton) -> bool {
        self.spec.is_side(side(button))
    }

    /// Whether `button` is the chord's anchor, the button the ladder watches.
    pub fn is_anchor(&self, button: NavButton) -> bool {
        self.spec.anchor == side(button)
    }
}

/// This crate's mouse buttons, mirrored into the ladder crate's own names.
const fn side(button: NavButton) -> SideButton {
    match button {
        NavButton::Left => SideButton::Left,
        NavButton::Middle => SideButton::Middle,
        NavButton::Right => SideButton::Right,
    }
}

/// CATIA V5's mouse: the middle button alone pans, the middle button plus a side button turns
/// the view, and letting the side button go with the middle one still down zooms. A bare middle
/// click brings the picked point to the middle of the view. A `const fn` so the preset table can
/// carry it in `NAV_PRESETS`.
pub const fn catia_chord() -> NavChord {
    NavChord { spec: solidcraft_nav_extras::CATIA, held: NavMode::Orbit, tapped: NavMode::Zoom }
}

/// The chord's anchor button, when the preset has one (for the drag ladder and the recentre).
pub(crate) fn chord_anchor(app: &SolveApp) -> Option<NavChord> {
    nav_preset(&app.preferences.nav).chord
}

/// Whether a right press now belongs to the preset's chord rather than to the context menu: the
/// chord's anchor (the middle button, in CATIA) is held and the right button is one of its sides.
/// Read before the menu opens, so holding the anchor turns the view instead of opening a menu
/// under the hand.
pub(crate) fn chord_takes_right(app: &SolveApp, middle_down: bool) -> bool {
    middle_down
        && nav_preset(&app.preferences.nav)
            .chord
            .is_some_and(|chord| chord.is_anchor(NavButton::Middle) && chord.is_side(NavButton::Right))
}

/// Whether the view's right-click menu must stand back this frame: under a two-button preset the
/// right button held with the anchor down turns the view, and no menu opens under the hand.
pub(crate) fn right_menu_blocked(app: &SolveApp, ui: &egui::Ui) -> bool {
    ui.input(|i| i.pointer.middle_down()) && chord_takes_right(app, true)
}

/// Which chord side button (if any) is down this frame, from the pointer's raw button states.
pub(crate) fn side_down(chord: Option<NavChord>, primary_down: bool, middle_down: bool, secondary_down: bool) -> bool {
    chord.is_some_and(|chord| {
        chord.spec.sides.iter().any(|side| match side {
            SideButton::Left => primary_down,
            SideButton::Middle => middle_down,
            SideButton::Right => secondary_down,
        })
    })
}

/// The mode of a drag of `button` this frame. The preset's chord ladder decides when the preset
/// carries one and `button` is its anchor; the plain binding table (with `nav_drag_keeps_mode`'s
/// memory of the starting mode) otherwise. `chord_reset` clears the ladder's latch when the drag
/// ends.
pub(crate) fn drag_mode(app: &mut SolveApp, button: NavButton, chord: Option<NavChord>, side_down: bool, mods: egui::Modifiers) -> Option<NavMode> {
    if let Some(chord) = chord.filter(|chord| chord.is_anchor(button)) {
        // A side button pressed at any time during the drag arms the zoom gear.
        if side_down {
            app.viewport.chord_latched = true;
        }
        let base = nav_mode(app, button, mods);
        return chord.mode(side_down, app.viewport.chord_latched, base);
    }
    let kept = app.viewport.button_nav.filter(|(held, _)| *held == button && app.preferences.nav_drag_keeps_mode);
    kept.map_or_else(|| nav_mode(app, button, mods), |(_, mode)| mode)
}

/// The anchor is up: the next drag starts in the anchor's own gear (pan).
pub(crate) fn chord_reset(app: &mut SolveApp) {
    app.viewport.chord_latched = false;
}

/// A middle click under a preset whose chord asks for it (CATIA): the point picked, or the middle
/// of the bodies in view over empty space, moves to the centre of the view. The camera's target
/// is what the view turns about, so the next orbit turns about that point. Only a click counts:
/// a drag pans or turns, and the second click of a double-click (Home or Fit) is left alone.
pub(crate) fn middle_click_recentre(app: &mut SolveApp, ui: &egui::Ui, inside: bool, rect: Rect) {
    let Some(chord) = chord_anchor(app).filter(|chord| chord.spec.recentre) else { return };
    // The anchor must be the middle button for a middle click to mean anything here.
    if chord.spec.anchor != SideButton::Middle {
        return;
    }
    let (pressed, released, pos, double) = ui.input(|i| {
        (
            i.pointer.button_pressed(egui::PointerButton::Middle),
            i.pointer.button_released(egui::PointerButton::Middle),
            i.pointer.latest_pos(),
            i.pointer.button_double_clicked(egui::PointerButton::Middle),
        )
    });
    // Where the button went down, kept here rather than read from egui's click: a synthetic drag
    // whose pointer jumps on its last frame is still classified as a click, and a click that
    // travelled is not one (#no-op).
    if pressed {
        app.viewport.chord_press = pos;
    }
    let Some(start) = app.viewport.chord_press else { return };
    if !released {
        return;
    }
    app.viewport.chord_press = None;
    let Some(at) = pos else { return };
    // A click that moved is a pan or a turn, and a double-click is Home or Fit.
    if !inside || double || start.distance(at) > CLICK_WOBBLE {
        return;
    }
    // The picked point, else the middle of the bodies in view: the same choice an orbit makes
    // when it turns about the cursor.
    let proj = projection(app, rect);
    let pivot = pivot_under(app, &proj, at);
    app.cancel_view_animation();
    app.viewport.orbit_pivot = Some(pivot);
    // `place` keeps `p` at the given place in the camera's frame: the frame's origin is the
    // middle of the view, so `p` lands there with the distance unchanged.
    app.cam.place(pivot, Vec3::ZERO);
}

/// A two-button preset (CATIA) reads as two extra legend rows: the anchor plus a side button,
/// held or tapped. A plain binding table cannot show them, and they are what the preset is
/// chosen for. Inserted ahead of the plain gestures, where the preset's headline acts belong.
pub(crate) fn legend_rows(app: &SolveApp, gestures: &mut Vec<(Vec<String>, String)>) {
    let Some(chord) = chord_anchor(app) else { return };
    let button = match chord.spec.anchor {
        SideButton::Left => "prefs.button_left",
        SideButton::Middle => "prefs.button_middle",
        SideButton::Right => "prefs.button_right",
    };
    let anchor = app.tr(button).to_owned();
    let side = app.tr("nav.chord.side").to_owned();
    gestures.insert(0, (vec![anchor.clone(), side.clone(), app.tr("nav.chord.hold").to_owned()], app.tr("prefs.gesture_orbit").to_owned()));
    gestures.insert(1, (vec![anchor, side, app.tr("nav.chord.tap").to_owned()], app.tr("prefs.gesture_zoom").to_owned()));
}

/// A scenario drag's `chord` / `chord_tap` parameters, as synthetic button events: `chord` holds
/// a second button down for the whole drag (CATIA's middle plus a side button, which turns the
/// view); `chord_tap` presses and lets go of that button at once, which changes gear to zoom
/// while the first button stays down.
pub(crate) fn synthetic_chord_press(app: &mut SolveApp, name: Option<&str>, tap: bool, pos: Pos2, modifiers: egui::Modifiers) {
    if let Some(side) = side_button(name) {
        app.synthetic.push(egui::Event::PointerButton { pos, button: side, pressed: true, modifiers });
        if tap {
            app.synthetic.push(egui::Event::PointerButton { pos, button: side, pressed: false, modifiers });
        }
    }
}

/// The held side button is let go before the first one, as a hand would (a tap has already been
/// let go, back where it was pressed).
pub(crate) fn synthetic_chord_release(app: &mut SolveApp, name: Option<&str>, tap: bool, pos: Pos2, modifiers: egui::Modifiers) {
    if let Some(side) = (!tap).then_some(()).and_then(|_| side_button(name)) {
        app.synthetic.push(egui::Event::PointerButton { pos, button: side, pressed: false, modifiers });
    }
}

/// A scenario parameter's button: "left", "right" or "middle".
fn side_button(name: Option<&str>) -> Option<egui::PointerButton> {
    match name {
        Some("right") => Some(egui::PointerButton::Secondary),
        Some("left") => Some(egui::PointerButton::Primary),
        Some("middle") => Some(egui::PointerButton::Middle),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::Modifiers;

    /// The CATIA preset carries the ladder, and only its anchor climbs it.
    #[test]
    fn the_catia_chord_changes_gear_under_a_held_anchor() {
        let catia = nav_preset("catia");
        let chord = catia.chord.expect("CATIA carries a chord");
        assert_eq!(chord.spec.anchor, SideButton::Middle);
        assert!(chord.is_side(NavButton::Left) && chord.is_side(NavButton::Right));
        assert!(chord.spec.recentre, "a middle click centres the view");
        // The anchor alone does what its binding says (pan).
        assert_eq!(chord.mode(false, false, Some(NavMode::Pan)), Some(NavMode::Pan));
        // A side button pressed and held turns the view.
        assert_eq!(chord.mode(true, false, Some(NavMode::Pan)), Some(NavMode::Orbit));
        // Let go with the anchor still down: zoom, and it stays zoom until pressed again.
        assert_eq!(chord.mode(false, true, Some(NavMode::Pan)), Some(NavMode::Zoom));
        assert_eq!(chord.mode(true, true, Some(NavMode::Pan)), Some(NavMode::Orbit));
        // The preset resolves through the same ladder, only for its anchor.
        assert_eq!(catia.chord_mode(NavButton::Middle, true, false, Modifiers::NONE), Some(NavMode::Orbit));
        assert_eq!(catia.chord_mode(NavButton::Right, true, false, Modifiers::NONE), None);
        // A one-button preset is untouched by any of it.
        assert_eq!(nav_preset("fusion").chord_mode(NavButton::Middle, true, false, Modifiers::NONE), None);
        assert!(nav_preset("fusion").chord.is_none());
    }

    /// `catia_chord()` is what the preset table carries, so a change to one shows in the other.
    #[test]
    fn the_preset_carries_the_const_chord() {
        let catia = nav_preset("catia").chord.expect("CATIA carries a chord");
        let built = catia_chord();
        assert_eq!(catia.spec.anchor, built.spec.anchor);
        assert_eq!(catia.held, built.held);
        assert_eq!(catia.tapped, built.tapped);
        assert_eq!(catia.spec.recentre, built.spec.recentre);
    }
}

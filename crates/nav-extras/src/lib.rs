//! Standalone navigation extensions: the two-button chord ladder behind the CATIA mouse preset.
//!
//! A chord is CATIA's way of driving a view with two buttons: one anchor stays down (the middle
//! one, which pans on its own) while a side button changes the drag's gear. Side button held, the
//! drag turns the view; side button let go with the anchor still down, the drag zooms; side
//! button pressed again, it turns again — the gear changes under a hand that never lifts the
//! anchor. A click of the anchor with no drag may recentre the view on the picked point.
//!
//! The ladder here is deliberately free of UI types: `SideButton` mirrors the host's button enum
//! rather than importing it, so the whole ladder is decided by booleans and tested without a
//! window. `ui-egui`'s `nav_extras` module maps `Gear` onto its own `NavMode` and owns everything
//! that touches the camera. See `docs/design/nav-extras-integration.md` for how the two meet,
//! and how to keep the join intact across upstream merges.

/// Which mouse button a chord speaks of. Mirrors `ui-egui`'s `prefs::NavButton` one to one; the
/// host maps between them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SideButton {
    Left,
    Middle,
    Right,
}

/// What a drag does this frame. `Anchor` hands the decision back to the host's own binding table
/// (CATIA's middle drag pans); `Held` and `Tapped` are the chord's own gears.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gear {
    /// No side button has touched the drag: the anchor's own binding decides.
    Anchor,
    /// A side button is down: the drag does what `held` says (CATIA: orbit).
    Held,
    /// A side button was pressed and let go, the anchor still down: the drag does what `tapped`
    /// says (CATIA: zoom) until a side button is pressed again.
    Tapped,
}

/// The shape of a chord: which button anchors the drag, which sides change its gear, and whether
/// a bare click of the anchor recentres the view. The gears themselves (`held` / `tapped`) stay
/// with the host, whose `NavMode` this crate does not know.
#[derive(Clone, Copy, Debug)]
pub struct ChordSpec {
    /// The button that must stay down (CATIA: the middle one).
    pub anchor: SideButton,
    /// The buttons that change the gear (CATIA: left or right, whichever is handier).
    pub sides: &'static [SideButton],
    /// A click of the anchor with no drag moves the picked point to the centre of the view.
    pub recentre: bool,
}

impl ChordSpec {
    /// Whether `button` is one of the chord's side buttons.
    pub fn is_side(&self, button: SideButton) -> bool {
        self.sides.contains(&button)
    }

    /// What the drag does this frame. `latched` remembers that a side button has been pressed
    /// since the anchor went down; it survives letting the side button go, and the host clears it
    /// when the anchor itself is let go — the next drag starts in the anchor's own gear.
    pub fn gear(&self, side_down: bool, latched: bool) -> Gear {
        if side_down {
            Gear::Held
        } else if latched {
            Gear::Tapped
        } else {
            Gear::Anchor
        }
    }
}

/// CATIA V5's mouse: the middle button alone pans, the middle button plus a side button turns
/// the view, and letting the side button go with the middle one still down zooms. A bare middle
/// click brings the picked point to the middle of the view.
pub const CATIA: ChordSpec = ChordSpec {
    anchor: SideButton::Middle,
    sides: &[SideButton::Left, SideButton::Right],
    recentre: true,
};

/// How far a pointer may move and still count as a click (points): egui's own click tolerance.
/// Used to tell a bare click (recentre) from a drag that pans or turns.
pub const CLICK_WOBBLE: f32 = 6.0;

#[cfg(test)]
mod tests {
    use super::*;

    /// The ladder, walked end to end: anchor alone, side held, side let go (the latch keeps the
    /// gear), side pressed again, and the anchor let go which resets the whole gesture.
    #[test]
    fn the_ladder_walks_anchor_held_tapped_and_back() {
        let chord = CATIA;
        // Nothing pressed yet: the anchor's own gear (pan, in the host's terms).
        assert_eq!(chord.gear(false, false), Gear::Anchor);
        // A side button goes down: turn the view.
        assert_eq!(chord.gear(true, false), Gear::Held);
        // Still held, now latched: still turning.
        assert_eq!(chord.gear(true, true), Gear::Held);
        // The side button is let go, the anchor still down: the latch keeps a gear, zoom now.
        assert_eq!(chord.gear(false, true), Gear::Tapped);
        // The side button pressed again: back to turning, without the anchor ever being lifted.
        assert_eq!(chord.gear(true, true), Gear::Held);
        // The anchor is let go: the host clears the latch, and the next drag starts over.
        assert_eq!(chord.gear(false, false), Gear::Anchor);
    }

    /// A side button anywhere in the ladder never changes what the anchor's own gear would be
    /// when no latch exists: chords add gears, they do not take the anchor's away.
    #[test]
    fn the_anchor_gear_survives_until_a_side_button_touches_the_drag() {
        for latched in [false, true] {
            assert_eq!(CATIA.gear(false, latched), if latched { Gear::Tapped } else { Gear::Anchor });
        }
    }

    /// CATIA's sides are the left and right buttons, and neither is the anchor.
    #[test]
    fn catia_sides_are_left_and_right() {
        assert!(CATIA.is_side(SideButton::Left));
        assert!(CATIA.is_side(SideButton::Right));
        assert!(!CATIA.is_side(SideButton::Middle));
        assert_eq!(CATIA.anchor, SideButton::Middle);
        assert!(CATIA.recentre);
    }

    /// A recentre-less chord is a legal shape: the ladder works the same, only the click loses
    /// its second meaning.
    #[test]
    fn a_chord_without_recentre_is_legal() {
        let chord = ChordSpec { recentre: false, ..CATIA };
        assert!(!chord.recentre);
        assert_eq!(chord.gear(true, false), Gear::Held);
    }

    /// The click tolerance is generous enough for a resting hand's tremor, and small enough that
    /// a deliberate drag is never mistaken for a click.
    #[test]
    fn the_click_tolerance_is_a_few_points() {
        assert!(CLICK_WOBBLE > 2.0 && CLICK_WOBBLE < 12.0);
    }
}

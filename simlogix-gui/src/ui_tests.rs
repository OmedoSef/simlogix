//! Tests that drive the whole application through a real `Ui`.
//!
//! # Why these exist
//!
//! Every other test here checks a function against its own arguments, and
//! they stayed green through a run of bugs that were never in a function: a
//! group drag that came apart by one frame's delta, copy/paste that never
//! fired, a view that framed the wrong thing, two gestures the simulation
//! mode was supposed to have taken away, and text painted over the windows.
//! Each lived in the *wiring* — which widget was asked what, in which order,
//! with which sense.
//!
//! Five of those is a category rather than bad luck, and it is the one
//! nothing else could reach. `egui_kittest` runs a real egui pass over the
//! real application, so a test can press where a person would and look at
//! what actually happened.
//!
//! # Why a child module of `app` rather than `tests/`
//!
//! An integration test sees only what is `pub`. Asserting on a component's
//! position or on what is selected would have meant publishing accessors
//! that exist for no other reason — inventing API to test with is how the
//! API stops describing the thing.
//!
//! Privacy is per module, and a sibling module is no better off, so this is
//! declared *inside* `app` with a `#[path]`. It keeps its own file, and the
//! fields stay private to everyone else.
//!
//! # What belongs here
//!
//! Behaviour that only exists once the pieces are assembled. A rule about one
//! function belongs beside that function, where it is cheaper to run and
//! easier to read; repeating it here would only mean two places to change it.

use egui_kittest::Harness;

use simlogix_core::ComponentId;

use super::SimLogixApp;
use crate::palette::ComponentKind;
use crate::toolbar;

/// Big enough that the canvas is not a sliver once the panels have taken
/// their share — these tests act in canvas coordinates.
const WINDOW: egui::Vec2 = egui::vec2(1280.0, 800.0);

fn harness() -> Harness<'static, SimLogixApp> {
    Harness::builder()
        .with_size(WINDOW)
        .build_ui_state(|ui, app| app.draw(ui), SimLogixApp::default())
}

/// Runs a fixed number of passes.
///
/// Not `Harness::run`, which repaints until the ui settles and gives up
/// after a few tries: this application asks for a repaint on *every* frame,
/// which is what keeps a placed clock ticking in real time, so it never
/// settles by that definition and never would.
///
/// Two passes rather than one, because egui delivers a press on the frame
/// after it is queued: the first pass hands the event over, the second is
/// where the widget under it responds.
fn step(harness: &mut Harness<'_, SimLogixApp>) {
    harness.run_steps(2);
}

#[test]
fn the_application_lays_itself_out_without_falling_over() {
    // The floor everything else stands on. A panel or a layer that panics
    // during layout says so here, rather than making some later test explain
    // it.
    let mut harness = harness();
    step(&mut harness);

    // And it really did run: a harness that quietly did nothing would pass
    // every assertion built on top of it.
    assert_eq!(harness.state().circuits.len(), 1);
}

/// Puts a component on the canvas and selects it, without going through the
/// canvas: placing by hand is covered by unit tests, and what these are here
/// for is what happens *after* something is selected.
fn place_and_select(harness: &mut Harness<'_, SimLogixApp>) -> ComponentId {
    let app = harness.state_mut();
    let id = app.place(ComponentKind::Led, egui::pos2(120.0, 120.0));
    app.selection.pick_component(id, false);
    id
}

fn press(harness: &mut Harness<'_, SimLogixApp>, key: egui::Key) {
    harness.input_mut().events.push(egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    });
}

#[test]
fn delete_removes_the_selected_component_on_a_schematic() {
    // The other half of the test below. Without it, "nothing happened" would
    // be just as true of a key that never reached the application at all.
    let mut harness = harness();
    place_and_select(&mut harness);
    step(&mut harness);

    press(&mut harness, egui::Key::Delete);
    step(&mut harness);

    assert!(harness.state().placed.is_empty());
}

#[test]
fn delete_does_nothing_while_the_simulation_view_is_showing() {
    let mut harness = harness();
    place_and_select(&mut harness);
    harness.state_mut().switch_view(toolbar::View::Simulation);
    // Selecting is still possible there — a component has to answer a click
    // or a switch could not be flipped — so this is a selection that a key
    // could act on, and mustn't.
    let id = place_and_select(&mut harness);
    step(&mut harness);

    press(&mut harness, egui::Key::Delete);
    step(&mut harness);

    assert!(harness.state().placed.iter().any(|p| p.id() == id));
}

#[test]
fn copying_and_pasting_puts_a_second_component_on_the_canvas() {
    // This never worked when it was first written: egui turns Ctrl+C and
    // Ctrl+V into `Event::Copy`/`Event::Paste` and never emits the key press,
    // so the shortcut the code was matching on could not fire. Nothing short
    // of running the real event loop would have said so.
    let mut harness = harness();
    place_and_select(&mut harness);
    step(&mut harness);

    harness.input_mut().events.push(egui::Event::Copy);
    step(&mut harness);

    // Round-tripped through the real clipboard text, because the marker in
    // it is what tells a fragment from a pasted URL.
    let fragment = harness
        .state()
        .clipboard
        .clone()
        .expect("copying puts a fragment on the clipboard");
    harness
        .input_mut()
        .events
        .push(egui::Event::Paste(fragment));
    step(&mut harness);

    assert_eq!(harness.state().placed.len(), 2);
}

#[test]
fn pasting_is_refused_while_the_simulation_view_is_showing() {
    let mut harness = harness();
    place_and_select(&mut harness);
    step(&mut harness);
    harness.input_mut().events.push(egui::Event::Copy);
    step(&mut harness);
    let fragment = harness.state().clipboard.clone().expect("copied");

    harness.state_mut().switch_view(toolbar::View::Simulation);
    harness
        .input_mut()
        .events
        .push(egui::Event::Paste(fragment));
    step(&mut harness);

    // Copying is allowed there and pasting is not: reading something out
    // changes nothing, adding something does.
    assert_eq!(harness.state().placed.len(), 1);
}

/// The three halves of a drag, kept apart so a test can look *during* one.
///
/// That matters more than it sounds: everything on the canvas snaps to the
/// grid when the button comes up, and snapping hides any error smaller than
/// a grid step. A drag that is wrong by one frame's worth of movement is
/// invisible once released, and that is precisely a bug this file exists to
/// catch — so the interesting assertions happen mid-flight.
///
/// Positions are in *canvas* coordinates, which is how the application
/// stores everything; the mapping to the screen comes from the frame just
/// drawn — see `SimLogixApp::screen_pos` for why it is recorded rather than
/// recomputed.
fn press_at(harness: &mut Harness<'_, SimLogixApp>, canvas: egui::Pos2) {
    let pos = harness.state().screen_pos(canvas);
    harness
        .input_mut()
        .events
        .push(egui::Event::PointerMoved(pos));
    step(harness);
    harness.input_mut().events.push(egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    step(harness);
}

/// Moves the pointer there through several positions, because egui reads a
/// drag from successive ones: a single jump is a press and a release
/// somewhere else.
///
/// Small steps on purpose. `interact_box` deliberately doesn't move anything
/// on the frame a drag *starts* — that is what makes the undo snapshot the
/// true pre-drag state — so with coarse steps a test loses a whole grid step
/// and reads it as the application being wrong.
/// `from` is passed rather than remembered: the event queue is drained every
/// frame, so by the time this runs there is nothing left in it to read the
/// pointer's last position out of.
fn move_to(harness: &mut Harness<'_, SimLogixApp>, from: egui::Pos2, canvas: egui::Pos2) {
    let target = harness.state().screen_pos(canvas);
    let from = harness.state().screen_pos(from);
    for index in 1..=DRAG_STEPS {
        let at = from + (target - from) * (index as f32 / DRAG_STEPS as f32);
        harness
            .input_mut()
            .events
            .push(egui::Event::PointerMoved(at));
        step(harness);
    }
}

fn release(harness: &mut Harness<'_, SimLogixApp>, canvas: egui::Pos2) {
    let pos = harness.state().screen_pos(canvas);
    harness.input_mut().events.push(egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    step(harness);
}

fn drag(harness: &mut Harness<'_, SimLogixApp>, from: egui::Pos2, to: egui::Pos2) {
    press_at(harness, from);
    move_to(harness, from, to);
    release(harness, to);
}

const DRAG_STEPS: usize = 16;

/// Where a component sits now.
fn position_of(harness: &Harness<'_, SimLogixApp>, id: ComponentId) -> egui::Pos2 {
    harness
        .state()
        .placed
        .iter()
        .find(|placed| placed.id() == id)
        .expect("still placed")
        .center()
}

#[test]
fn dragging_a_component_moves_it() {
    // Proves the coordinate mapping before anything is built on it: if
    // `screen_pos` were wrong, the press would land on empty canvas and this
    // would fail rather than the tests below failing mysteriously.
    let mut harness = harness();
    let at = egui::pos2(200.0, 200.0);
    let id = harness.state_mut().place(ComponentKind::Led, at);
    step(&mut harness);

    drag(&mut harness, at, egui::pos2(280.0, 200.0));

    assert_eq!(position_of(&harness, id), egui::pos2(280.0, 200.0));
}

#[test]
fn dragging_one_of_several_selected_components_carries_the_others() {
    // It once came apart by exactly one frame's delta: the grabbed component
    // deliberately doesn't move on the frame a drag starts, so that the undo
    // snapshot is the true pre-drag state, while the rest of the selection
    // was being moved from the frame the pointer first reported a drag.
    let mut harness = harness();
    let (first, second) = (egui::pos2(200.0, 200.0), egui::pos2(200.0, 280.0));
    let (a, b) = {
        let app = harness.state_mut();
        let a = app.place(ComponentKind::Led, first);
        let b = app.place(ComponentKind::Led, second);
        app.selection.pick_component(a, false);
        app.selection.pick_component(b, true);
        (a, b)
    };
    step(&mut harness);

    // Looked at *mid-drag*, before the button comes up: on release every
    // position snaps to the grid, and one frame's worth of drift is smaller
    // than a grid step — so a released drag would come out equal either way
    // and this test would pass while the bug was back.
    press_at(&mut harness, first);
    move_to(&mut harness, first, egui::pos2(300.0, 200.0));

    let moved_a = position_of(&harness, a) - first;
    let moved_b = position_of(&harness, b) - second;
    assert_ne!(moved_a, egui::Vec2::ZERO, "the grabbed one moved");
    assert_eq!(moved_a, moved_b, "and the rest of the selection by as much");
    release(&mut harness, egui::pos2(300.0, 200.0));
}

#[test]
fn dragging_a_component_does_nothing_in_the_simulation_view() {
    let mut harness = harness();
    let at = egui::pos2(200.0, 200.0);
    let id = harness.state_mut().place(ComponentKind::Led, at);
    harness.state_mut().switch_view(toolbar::View::Simulation);
    step(&mut harness);

    drag(&mut harness, at, egui::pos2(300.0, 260.0));

    assert_eq!(position_of(&harness, id), at);
}

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

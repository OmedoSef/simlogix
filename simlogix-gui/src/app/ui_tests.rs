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

use egui_kittest::kittest::{self, Queryable};
use egui_kittest::Harness;

use simlogix_core::{ComponentId, Level};

use super::SimLogixApp;
use super::WireEndpoint;
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

#[test]
fn stepping_stops_the_simulation_and_advances_exactly_one_tick() {
    let mut harness = harness();
    step(&mut harness);
    assert!(harness.state().running, "it runs on its own to begin with");

    // The first press is what stops it. Until then time moves with the
    // frames, so there is no fixed number to compare against.
    press(&mut harness, egui::Key::F10);
    step(&mut harness);
    assert!(!harness.state().running, "a step stops it");

    // Stopped means stopped: frames go by and the clock does not.
    let stopped_at = harness.state().circuit.now();
    step(&mut harness);
    step(&mut harness);
    assert_eq!(harness.state().circuit.now(), stopped_at);

    // And now a step is exactly one tick — one propagation delay.
    press(&mut harness, egui::Key::F10);
    step(&mut harness);
    assert_eq!(harness.state().circuit.now(), stopped_at + 1);
}

#[test]
fn sixty_steps_reach_a_clock() {
    let mut harness = harness();
    let clock = harness
        .state_mut()
        .place(ComponentKind::Clock, egui::pos2(200.0, 200.0));
    step(&mut harness);

    let level = |harness: &Harness<'_, SimLogixApp>| {
        let app = harness.state();
        app.circuit
            .signal_at(app.circuit.pins(clock)[0].net)
            .only_level()
    };

    press(&mut harness, egui::Key::F10);
    step(&mut harness);
    let before = level(&harness);

    // A clock beats once a second at sixty ticks a second, so it has to
    // turn over within sixty of them wherever the pause happened to land.
    for _ in 0..60 {
        press(&mut harness, egui::Key::F10);
        step(&mut harness);
    }
    assert_ne!(
        level(&harness),
        before,
        "sixty ticks and the clock never moved"
    );
}

#[test]
fn skipping_lands_on_the_next_event_rather_than_the_next_tick() {
    let mut harness = harness();
    harness
        .state_mut()
        .place(ComponentKind::Clock, egui::pos2(200.0, 200.0));
    step(&mut harness);

    // Stop somewhere, then read where the next thing is actually due.
    press(&mut harness, egui::Key::F10);
    step(&mut harness);
    let due = harness
        .state()
        .circuit
        .next_event_tick()
        .expect("a clock always has a next beat");
    assert!(
        due > harness.state().circuit.now() + 1,
        "the point of skipping is that it is further than one tick"
    );

    press_with(&mut harness, egui::Key::F10, egui::Modifiers::SHIFT);
    step(&mut harness);
    assert_eq!(harness.state().circuit.now(), due);
}

#[test]
fn the_speed_multiplier_moves_logical_time_against_the_frames() {
    // Frames are the same either way; what changes is how much time each
    // one is worth.
    let elapsed = |speed: f32| {
        let mut harness = harness();
        harness.state_mut().speed = speed;
        step(&mut harness);
        let from = harness.state().circuit.now();
        for _ in 0..10 {
            step(&mut harness);
        }
        harness.state().circuit.now() - from
    };

    let slow = elapsed(0.25);
    let fast = elapsed(4.0);
    assert!(
        fast > slow,
        "four times speed covered {fast} ticks against {slow} at a quarter"
    );
}

#[test]
fn a_clock_step_lands_on_an_edge_of_the_clock() {
    let mut harness = harness();
    let clock = harness
        .state_mut()
        .place(ComponentKind::Clock, egui::pos2(200.0, 200.0));
    step(&mut harness);

    let level = |harness: &Harness<'_, SimLogixApp>| {
        let app = harness.state();
        app.circuit
            .signal_at(app.circuit.pins(clock)[0].net)
            .only_level()
    };

    press(&mut harness, egui::Key::F10);
    step(&mut harness);
    let before = level(&harness);

    // One press, however many ticks that turns out to be — which is the
    // point, since the answer depends on where the pause landed.
    press_with(&mut harness, egui::Key::F10, egui::Modifiers::COMMAND);
    step(&mut harness);
    assert_ne!(level(&harness), before, "a clock step must reach an edge");
}

#[test]
fn a_clock_step_drives_a_port_when_that_is_where_the_clock_comes_from() {
    // A circuit drawn to sit inside another has its clock arriving on a
    // port, so there is no `Clock` to advance to — you are its clock.
    let mut harness = harness();
    let port = harness
        .state_mut()
        .place(ComponentKind::InputPort, egui::pos2(200.0, 200.0));
    step(&mut harness);

    let level = |harness: &Harness<'_, SimLogixApp>| {
        let app = harness.state();
        app.circuit
            .signal_at(app.circuit.pins(port)[0].net)
            .only_level()
    };

    press_with(&mut harness, egui::Key::F10, egui::Modifiers::COMMAND);
    step(&mut harness);
    let first = level(&harness);
    assert_eq!(first, Level::High, "an undriven port starts the cycle high");

    // And back, because a cycle is two levels: undriven is a third position
    // of the switch, not part of one.
    press_with(&mut harness, egui::Key::F10, egui::Modifiers::COMMAND);
    step(&mut harness);
    assert_eq!(level(&harness), Level::Low);
}

#[test]
fn the_simulation_row_has_a_run_pause_button() {
    let mut harness = harness();
    harness
        .state_mut()
        .switch_view(crate::toolbar::View::Simulation);
    step(&mut harness);
    assert!(harness.state().running);

    // Found by its tooltip, which is the menu's own label — the same two
    // words, so there is only one translation to keep right.
    let label = |harness: &Harness<'_, SimLogixApp>| {
        let app = harness.state();
        let strings = crate::i18n::Strings::for_language(app.language);
        if app.running {
            strings.menu_simulation_pause
        } else {
            strings.menu_simulation_run
        }
    };

    let pause = label(&harness).to_string();
    harness.get_by_label(&pause).click();
    step(&mut harness);
    assert!(!harness.state().running, "the button stopped it");

    let run = label(&harness).to_string();
    harness.get_by_label(&run).click();
    step(&mut harness);
    assert!(harness.state().running, "and started it again");
}

#[test]
fn the_inspector_opens_and_names_what_is_driving_a_net() {
    let mut harness = harness();
    let source = harness
        .state_mut()
        .place(ComponentKind::InputPort, egui::pos2(200.0, 200.0));
    harness.state_mut().selection.pick_component(source, false);
    step(&mut harness);
    assert!(!harness.state().show_inspector);

    press(&mut harness, egui::Key::F12);
    step(&mut harness);
    assert!(harness.state().show_inspector, "F12 opens it");

    // The line the whole window exists for: not what the net resolved to,
    // but who put that there. Found on screen rather than in the state,
    // since drawing it is the part that could be wrong.
    let strings = crate::i18n::Strings::for_language(harness.state().language);
    // The label *and* the pin together: searching for the kind's name alone
    // found it in the palette, so the first version of this passed with the
    // naming ripped out.
    let row = format!(
        "{} · {}",
        strings.component_kind_label(&ComponentKind::InputPort),
        strings.inspector_pin.replace("{}", "0"),
    );
    assert!(
        harness.get_all_by_label_contains(&row).next().is_some(),
        "expected a row naming what drives the net, found none matching {row:?}"
    );

    press(&mut harness, egui::Key::F12);
    step(&mut harness);
    assert!(!harness.state().show_inspector, "and closes it again");
}

#[test]
fn hovering_a_bus_says_how_wide_it_is_and_a_plain_wire_says_nothing() {
    let mut harness = harness();
    let (from, to) = (egui::pos2(0.0, 0.0), egui::pos2(200.0, 0.0));
    let source = harness.state_mut().place(ComponentKind::InputPort, from);
    let sink = harness.state_mut().place(ComponentKind::OutputPort, to);
    harness.state_mut().add_wire(
        WireEndpoint::Pin(source, 0),
        WireEndpoint::Pin(sink, 0),
        Vec::new(),
    );
    harness.state_mut().rebuild_nets();
    step(&mut harness);

    let strings = crate::i18n::Strings::for_language(harness.state().language);
    let one_bit = strings.inspector_bits.replace("{}", "1");
    let four_bits = strings.inspector_bits.replace("{}", "4");

    // Over the middle of the wire, which is one bit wide.
    move_to(&mut harness, from, egui::pos2(100.0, 0.0));
    step(&mut harness);
    assert!(
        harness
            .query_all(kittest::By::new().label_contains(&one_bit))
            .next()
            .is_none(),
        "a plain wire's width is the default; saying it over every wire is noise"
    );

    for id in [source, sink] {
        let app = harness.state_mut();
        let placed = app
            .placed
            .iter_mut()
            .find(|placed| placed.id() == id)
            .expect("just placed");
        let mut properties = placed.properties().clone();
        properties.width = Some(4);
        placed.set_properties(properties);
    }
    harness.state_mut().rebuild_nets();
    move_to(&mut harness, from, egui::pos2(100.0, 0.0));
    step(&mut harness);
    assert!(
        harness
            .query_all(kittest::By::new().label_contains(&four_bits))
            .next()
            .is_some(),
        "a bus should say how wide it is without being selected"
    );
}

#[test]
fn the_inspector_shows_a_reader_that_disagrees_about_the_width() {
    // The case that is otherwise invisible: a reader puts nothing on the
    // net, so nothing about it shows in what the net carries. Romain found
    // it by wiring a two-bit output to a four-bit bus and seeing no
    // complaint anywhere.
    let mut harness = harness();
    let wide = harness
        .state_mut()
        .place(ComponentKind::InputPort, egui::pos2(0.0, 0.0));
    let narrow = harness
        .state_mut()
        .place(ComponentKind::OutputPort, egui::pos2(200.0, 0.0));
    {
        let app = harness.state_mut();
        app.add_wire(
            WireEndpoint::Pin(wide, 0),
            WireEndpoint::Pin(narrow, 0),
            Vec::new(),
        );
        for (id, bits) in [(wide, 4), (narrow, 2)] {
            let placed = app
                .placed
                .iter_mut()
                .find(|placed| placed.id() == id)
                .expect("just placed");
            let mut properties = placed.properties().clone();
            properties.width = Some(bits);
            placed.set_properties(properties);
        }
        app.rebuild_nets();
    }

    let strings = crate::i18n::Strings::for_language(harness.state().language);
    let named = harness.state().named_components(strings);
    let report = crate::inspector::report(strings, &harness.state().circuit, &named);

    // Not the net's number, which is handed out afresh on every rebuild.
    assert!(report.contains("· 4 bits ·"), "{report}");
    assert!(
        report.contains("2 bits · reads  (width mismatch)"),
        "{report}"
    );
}

#[test]
fn the_bug_report_carries_the_build_and_what_drives_each_net() {
    let mut harness = harness();
    let source = harness
        .state_mut()
        .place(ComponentKind::InputPort, egui::pos2(200.0, 200.0));
    step(&mut harness);

    // Named for real rather than by hand: what the report has to carry is
    // the name *the user gave*, so setting the property is the thing worth
    // checking — building the row here would prove only that a string put
    // into a struct comes back out of it.
    harness.state_mut().set_component_properties(
        source,
        crate::properties::Properties {
            name: Some("CLK".to_string()),
            ..Default::default()
        },
    );
    step(&mut harness);

    let app = harness.state();
    let strings = crate::i18n::Strings::for_language(app.language);
    let named = app.named_components(strings);
    let report = crate::inspector::report(strings, &app.circuit, &named);

    // The first three questions any report has to answer before anyone can
    // help: which build, on what, and what the engine thinks.
    assert!(report.contains(env!("CARGO_PKG_VERSION")), "{report}");
    assert!(report.contains(std::env::consts::OS), "{report}");
    assert!(report.contains("CLK · pin 0"), "{report}");
    // And never the drawing itself: a circuit is the user's, and a copy
    // button should not quietly hand it over.
    assert!(!report.contains("InputPort"), "{report}");
}

fn press_with(harness: &mut Harness<'_, SimLogixApp>, key: egui::Key, modifiers: egui::Modifiers) {
    harness.input_mut().events.push(egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    });
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

/// A press and a release in the same place.
fn click_at(harness: &mut Harness<'_, SimLogixApp>, canvas: egui::Pos2) {
    press_at(harness, canvas);
    release(harness, canvas);
}

#[test]
fn a_tri_state_source_can_be_clicked_into_letting_go() {
    // The reason the component exists: without a third position there is
    // no way to leave a bus undriven and watch something else answer for
    // it, which is the only honest way to test a bidirectional buffer.
    let mut harness = harness();
    let at = egui::pos2(200.0, 200.0);
    let id = harness.state_mut().place(ComponentKind::TriStateSource, at);
    // Driving by hand is a simulation gesture: in the schematic a click on
    // one of these selects it, so that setting a width or a name does not
    // also poke the circuit.
    harness
        .state_mut()
        .switch_view(crate::toolbar::View::Simulation);
    step(&mut harness);

    let signal = |harness: &Harness<'_, SimLogixApp>| {
        let app = harness.state();
        let net = app.circuit.pins(id)[0].net;
        app.circuit.signal_at(net).only_level()
    };
    // Nothing else is on this net, so what it reads *is* what the source
    // is putting on it.
    assert_eq!(signal(&harness), Level::Unknown);

    // Low then high: a click steps the value up, which on a plain wire is
    // the switch it has always been — the order is the one thing that
    // changed when the cycle stopped rebuilding the value out of all-ones.
    click_at(&mut harness, at);
    assert_eq!(signal(&harness), Level::Low);
    click_at(&mut harness, at);
    assert_eq!(signal(&harness), Level::High);

    // And round to letting go, without a property having to be set for it.
    // A port would have gone back to high here: there the number of states
    // is declared, because it is a promise to whatever drives the pin from
    // outside. A source has nothing to declare.
    click_at(&mut harness, at);
    assert_eq!(signal(&harness), Level::Unknown);
}

fn secondary_click_at(harness: &mut Harness<'_, SimLogixApp>, canvas: egui::Pos2) {
    let pos = harness.state().screen_pos(canvas);
    harness
        .input_mut()
        .events
        .push(egui::Event::PointerMoved(pos));
    step(harness);
    for pressed in [true, false] {
        harness.input_mut().events.push(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
        step(harness);
    }
}

/// Two components with a wire between them, bent through one waypoint.
fn wired_pair(harness: &mut Harness<'_, SimLogixApp>) -> egui::Pos2 {
    let waypoint = egui::pos2(300.0, 200.0);
    let app = harness.state_mut();
    let a = app.place(ComponentKind::Switch, egui::pos2(200.0, 200.0));
    let b = app.place(ComponentKind::Led, egui::pos2(400.0, 300.0));
    app.add_wire(
        WireEndpoint::Pin(a, 0),
        WireEndpoint::Pin(b, 0),
        vec![waypoint],
    );
    step(harness);
    waypoint
}

#[test]
fn dragging_a_wires_waypoint_reshapes_it() {
    let mut harness = harness();
    let waypoint = wired_pair(&mut harness);

    drag(&mut harness, waypoint, egui::pos2(300.0, 120.0));

    assert_eq!(
        harness.state().wires[0].waypoints[0],
        egui::pos2(300.0, 120.0)
    );
}

#[test]
fn right_clicking_a_wire_cuts_it_in_two() {
    let mut harness = harness();
    let (a, b) = {
        let app = harness.state_mut();
        (
            app.place(ComponentKind::Switch, egui::pos2(200.0, 200.0)),
            app.place(ComponentKind::Led, egui::pos2(460.0, 200.0)),
        )
    };
    // Two waypoints, so the route has a segment that is neither its first
    // nor its last. Cutting at the very start leaves nothing before the cut
    // and so gives one piece rather than two — which is correct, and not
    // what this test is about.
    harness.state_mut().add_wire(
        WireEndpoint::Pin(a, 0),
        WireEndpoint::Pin(b, 0),
        vec![egui::pos2(300.0, 200.0), egui::pos2(360.0, 200.0)],
    );
    step(&mut harness);
    assert_eq!(harness.state().wires.len(), 1);

    secondary_click_at(&mut harness, egui::pos2(330.0, 200.0));

    let wires = &harness.state().wires;
    assert_eq!(wires.len(), 2, "cut into two pieces");
    // Each piece keeps the end it already had and gains a loose one where
    // the cut fell, so neither is left attached to nothing.
    assert!(wires
        .iter()
        .all(|wire| matches!(wire.from, WireEndpoint::Free(_))
            || matches!(wire.to, WireEndpoint::Free(_))));
}

#[test]
fn a_rubber_band_selects_everything_it_touches() {
    let mut harness = harness();
    let (a, b) = {
        let app = harness.state_mut();
        (
            app.place(ComponentKind::Led, egui::pos2(240.0, 200.0)),
            app.place(ComponentKind::Led, egui::pos2(240.0, 300.0)),
        )
    };
    step(&mut harness);

    // Starts on empty canvas above and left of both, and stops *inside* the
    // second one rather than past it: the band takes what it overlaps, not
    // only what it swallows whole, and a long component reaching out of the
    // sweep would otherwise be impossible to catch.
    drag(
        &mut harness,
        egui::pos2(140.0, 140.0),
        egui::pos2(240.0, 300.0),
    );

    let selection = &harness.state().selection;
    assert!(selection.components.contains(&a), "the one it covered");
    assert!(
        selection.components.contains(&b),
        "and the one it only touched"
    );
}

#[test]
fn a_queued_component_is_dropped_where_the_canvas_is_clicked() {
    // This broke once for a reason no unit test could see: a full-canvas
    // `ui.interact` for the rubber band covered the `Scene`'s own background
    // response, and placement goes through exactly that.
    let mut harness = harness();
    harness.state_mut().tool = toolbar::Tool::Place(ComponentKind::Led);
    step(&mut harness);

    click_at(&mut harness, egui::pos2(260.0, 220.0));

    let placed = &harness.state().placed;
    assert_eq!(placed.len(), 1);
    assert_eq!(placed[0].center(), egui::pos2(260.0, 220.0));
}

#[test]
fn switching_circuit_brings_the_drawing_into_view() {
    // The logic here was right and the wiring threw the result away:
    // `scene_rect` is copied into a local before `Scene::show` and written
    // back after, so assigning the *field* had the value computed and
    // discarded every frame. Only a real frame shows that.
    let mut harness = harness();
    let far = egui::pos2(2400.0, 1800.0);
    {
        let app = harness.state_mut();
        app.place(ComponentKind::Led, far);
        app.create_circuit(String::new());
    }
    step(&mut harness);

    harness.state_mut().switch_to(0);
    step(&mut harness);

    assert!(
        harness.state().scene_rect.contains(far),
        "the camera should be looking at the drawing, not where it was before"
    );
}

#[test]
fn circuit_labels_are_painted_behind_the_floating_windows() {
    // Opening About over a schematic once printed the circuit's text across
    // it: the label layer was `Order::Foreground`, which is where menus and
    // popups go — above every window.
    //
    // Checked by reading egui's layer ordering rather than by rendering and
    // comparing images. The bug *was* an ordering fact, so this asserts the
    // thing itself; and a pixel snapshot of text could not hold across the
    // three platforms CI runs on, where rasterisation differs.
    let mut harness = harness();
    {
        let app = harness.state_mut();
        // A probe draws its readout, which is what puts a label layer on
        // screen at all.
        app.place(ComponentKind::Probe, egui::pos2(200.0, 200.0));
        app.show_about = true;
    }
    step(&mut harness);

    let canvas = harness
        .state()
        .canvas_layer
        .expect("the canvas was drawn this frame");
    // Asked through the same function the drawing uses, so the test cannot
    // come to disagree with it about which layer that is.
    let labels = crate::symbol::TextLayer::layer_id(canvas);

    // Back to front, top last.
    let order: Vec<egui::LayerId> = harness.ctx.memory(|m| m.layer_ids().collect());
    let position = |layer: egui::LayerId| {
        order
            .iter()
            .position(|held| *held == layer)
            .expect("painted this frame")
    };

    // Stated without naming the window: egui registers it under an id
    // derived from its title in a way that is its own business, and copying
    // that into a test would be pinning an implementation detail rather than
    // the rule. A floating window is any `Order::Middle` layer, and there is
    // one here only because About is open.
    let first_window = order
        .iter()
        .position(|layer| layer.order == egui::Order::Middle)
        .expect("About is open, so a floating window exists");

    assert!(
        position(labels) < first_window,
        "labels belong under the windows that cover them"
    );
    // The other end of the same rule: still above the drawing, or the fix
    // would have pushed them behind the circuit instead.
    assert!(position(canvas) < position(labels));
}

#[test]
fn the_wire_tool_does_not_start_a_wire_from_the_middle_of_a_component() {
    // Clicking a gate while wiring used to begin a wire at a loose point
    // under it, which is not something anyone means: a component's pins are
    // the way in. The click selects it instead, as it would with any tool.
    let mut harness = harness();
    let at = egui::pos2(240.0, 200.0);
    let id = harness.state_mut().place(ComponentKind::Led, at);
    harness.state_mut().tool = toolbar::Tool::Wire;
    step(&mut harness);

    click_at(&mut harness, at);

    assert!(
        harness.state().wiring_from.is_none(),
        "no wire should have been started"
    );
    assert!(harness.state().selection.components.contains(&id));
}

#[test]
fn the_wire_tool_still_starts_a_wire_on_empty_canvas() {
    // The other half: the tool exists so a wire can be drawn ahead of what
    // it will connect to, and a fix that simply stopped clicks from starting
    // wires would pass the test above.
    let mut harness = harness();
    harness.state_mut().tool = toolbar::Tool::Wire;
    step(&mut harness);

    click_at(&mut harness, egui::pos2(240.0, 200.0));

    assert!(harness.state().wiring_from.is_some());
}

#[test]
fn renaming_starts_with_the_name_selected() {
    // Renaming almost always means replacing — either the name you want to
    // change, or a generated one the tree just made up — so the first
    // keystroke should take the place of what is there rather than being
    // appended to it.
    let mut harness = harness();
    let before = harness.state().circuits[0].name.clone();
    harness.state_mut().renaming = Some((
        crate::circuit_tree::RenameTarget::Circuit(0),
        before.clone(),
    ));
    // Two passes: the field appears, then it takes focus and the selection.
    step(&mut harness);
    step(&mut harness);

    harness
        .input_mut()
        .events
        .push(egui::Event::Text("X".to_string()));
    step(&mut harness);

    let typed = harness
        .state()
        .renaming
        .as_ref()
        .map(|(_, buffer)| buffer.clone())
        .expect("still renaming");
    assert_eq!(typed, "X", "typed over {before:?} rather than into it");
}

#[test]
fn a_wide_readout_grows_its_box_and_a_one_bit_one_does_not() {
    // Romain's screenshot: a 32-bit value hanging out of both sides of a
    // port and out of a probe. A symbol is one grid box whatever it shows,
    // so the box has to follow the readout.
    let mut harness = harness();
    let port = harness
        .state_mut()
        .place(ComponentKind::InputPort, egui::pos2(0.0, 0.0));
    step(&mut harness);

    let narrow = harness.state().placed[0].rect().width();
    assert_eq!(
        narrow,
        crate::canvas::BOX_SIZE.x,
        "a one-bit port is the box it has always been"
    );

    harness.state_mut().set_component_properties(
        port,
        crate::properties::Properties {
            width: Some(32),
            ..Default::default()
        },
    );
    step(&mut harness);

    let wide = harness.state().placed[0].rect().width();
    assert!(
        wide > narrow,
        "a 32-bit readout needs more room than a one-bit one: {wide} against {narrow}"
    );
    // And in whole grid steps, or the pins stop landing on the dots.
    assert_eq!(
        wide % crate::canvas::GRID_SPACING,
        0.0,
        "grown by whole grid steps, not by however many points the text took"
    );

    // Binary is wider still, since the same value takes four times the
    // characters — the box follows the base as much as the width.
    harness.state_mut().set_component_properties(
        port,
        crate::properties::Properties {
            width: Some(32),
            base: Some(crate::properties::NumberBase::Binary),
            ..Default::default()
        },
    );
    step(&mut harness);
    assert!(
        harness.state().placed[0].rect().width() > wide,
        "binary takes four times the characters of hexadecimal"
    );
}

#[test]
fn clicking_a_bus_port_steps_its_value_instead_of_wiping_it() {
    // Romain's: a click rebuilt the value out of all-ones, so it slammed
    // every bit alike and threw away whatever had been typed. Handy on a
    // plain wire, where the value *is* the position; destructive on a bus.
    let mut harness = harness();
    let at = egui::pos2(200.0, 200.0);
    let id = harness.state_mut().place(ComponentKind::InputPort, at);
    harness.state_mut().set_component_properties(
        id,
        crate::properties::Properties {
            width: Some(4),
            ..Default::default()
        },
    );
    harness
        .state_mut()
        .switch_view(crate::toolbar::View::Simulation);
    step(&mut harness);

    let value = |harness: &Harness<'_, SimLogixApp>| {
        harness.state().placed[0]
            .hand_set_level()
            .expect("a driving port has a level")
            .get()
    };

    // Five clicks from nothing: down onto zero, then a step each time.
    for expected in 0..=3u64 {
        click_at(&mut harness, at);
        assert_eq!(
            value(&harness),
            simlogix_core::PortDrive::Driving(expected),
            "after {} clicks",
            expected + 1
        );
    }
}

#[test]
fn the_value_panel_stays_live_while_the_simulation_view_is_showing() {
    // The whole reason the value was split out of the properties: it is
    // runtime state, not something the document holds. Greying it along
    // with them undid that distinction, in the one mode it is for.
    let mut harness = harness();
    let id = harness
        .state_mut()
        .place(ComponentKind::InputPort, egui::pos2(200.0, 200.0));
    harness.state_mut().selection.pick_component(id, false);
    harness
        .state_mut()
        .switch_view(crate::toolbar::View::Simulation);
    step(&mut harness);

    let drive = |harness: &Harness<'_, SimLogixApp>| {
        harness.state().placed[0]
            .hand_set_level()
            .expect("a driving port has a level")
            .get()
    };
    assert_eq!(drive(&harness), simlogix_core::PortDrive::Undriven);

    // Clicked for real, through the widget: a disabled checkbox answers a
    // click with nothing, which is exactly what this has to rule out.
    let strings = crate::i18n::Strings::for_language(harness.state().language);
    harness.get_by_label(strings.value_driving).click();
    step(&mut harness);

    assert_eq!(
        drive(&harness),
        simlogix_core::PortDrive::Driving(0),
        "the value panel answered a click while the simulation was showing"
    );
}

#[test]
fn a_click_in_the_schematic_selects_a_port_without_driving_it() {
    // The other half of the rule, and the reason for it: a click in the
    // schematic is how a port is picked to set its width, its name or its
    // base — and driving on that same gesture poked the circuit every time.
    let mut harness = harness();
    let at = egui::pos2(200.0, 200.0);
    let id = harness.state_mut().place(ComponentKind::InputPort, at);
    step(&mut harness);

    click_at(&mut harness, at);
    step(&mut harness);

    assert!(
        harness.state().selection.components.contains(&id),
        "the click picked the port"
    );
    assert_eq!(
        harness.state().placed[0]
            .hand_set_level()
            .expect("a driving port has a level")
            .get(),
        simlogix_core::PortDrive::Undriven,
        "and left what it carries alone"
    );
}

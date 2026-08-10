//! Unit tests for the application's own logic.
//!
//! Its own file for the same reason as the rest of this folder: `app.rs` was
//! seven thousand lines, and a thousand of them were these. A child module,
//! so the private fields they assert on stay private to everyone else.

use super::*;

#[test]
fn creating_a_circuit_opens_it_without_disturbing_the_others() {
    let mut app = SimLogixApp::default();
    app.place(ComponentKind::Button, egui::pos2(40.0, 40.0));

    app.create_circuit(String::new());

    assert_eq!(app.circuits.len(), 2);
    assert_eq!(app.active, 1);
    // The new circuit starts empty...
    assert!(app.placed.is_empty());
    // ...and the one left behind kept what was drawn in it.
    assert_eq!(app.circuits[0].components.len(), 1);
}

#[test]
fn switching_keeps_each_circuit_to_its_own_layout() {
    let mut app = SimLogixApp::default();
    app.place(ComponentKind::Button, egui::pos2(40.0, 40.0));
    app.create_circuit(String::new());
    app.place(ComponentKind::Led, egui::pos2(80.0, 80.0));

    app.switch_to(0);
    assert_eq!(app.placed.len(), 1);
    assert_eq!(app.placed[0].kind(), ComponentKind::Button);

    app.switch_to(1);
    assert_eq!(app.placed.len(), 1);
    assert_eq!(app.placed[0].kind(), ComponentKind::Led);
}

#[test]
fn a_new_circuit_never_takes_a_name_already_in_use() {
    let mut app = SimLogixApp::default();
    app.create_circuit(String::new());
    app.create_circuit(String::new());

    let names: Vec<&str> = app
        .circuits
        .iter()
        .map(|circuit| circuit.name.as_str())
        .collect();
    let distinct: std::collections::HashSet<&&str> = names.iter().collect();
    assert_eq!(
        names.len(),
        distinct.len(),
        "duplicate name among {names:?}"
    );
}

#[test]
fn deleting_the_open_circuit_falls_onto_the_one_taking_its_place() {
    let mut app = SimLogixApp::default();
    app.create_circuit(String::new());
    app.create_circuit(String::new());
    app.switch_to(1);

    app.delete_circuit(1);

    assert_eq!(app.circuits.len(), 2);
    // Index 1 now holds what used to sit at 2.
    assert_eq!(app.active, 1);
}

#[test]
fn deleting_a_circuit_before_the_open_one_keeps_the_same_one_open() {
    let mut app = SimLogixApp::default();
    app.create_circuit(String::new());
    app.place(ComponentKind::Led, egui::pos2(80.0, 80.0));
    let open = app.circuits[app.active].name.clone();

    app.delete_circuit(0);

    assert_eq!(app.active, 0);
    assert_eq!(app.circuits[0].name, open);
    assert_eq!(app.placed.len(), 1);
}

#[test]
fn the_last_circuit_cannot_be_deleted() {
    let mut app = SimLogixApp::default();
    app.delete_circuit(0);
    assert_eq!(app.circuits.len(), 1);
}

#[test]
fn renaming_onto_a_name_already_in_use_is_refused() {
    let mut app = SimLogixApp::default();
    let taken = app.circuits[0].name.clone();
    app.create_circuit(String::new());

    app.rename_circuit(1, &taken);

    assert_ne!(app.circuits[1].name, taken);
    assert!(app.error.is_some(), "the clash should be reported");
}

#[test]
fn undoing_a_new_circuit_goes_back_to_the_project_before_it() {
    let mut app = SimLogixApp::default();
    app.place(ComponentKind::Button, egui::pos2(40.0, 40.0));
    app.create_circuit(String::new());

    app.undo();

    assert_eq!(app.circuits.len(), 1);
    assert_eq!(app.placed.len(), 1);
}

#[test]
fn the_library_is_named_after_the_file_once_and_then_stops_following_it() {
    let mut app = SimLogixApp::default();
    assert!(app.library.is_empty());

    app.name_library_after(std::path::Path::new("/tmp/cpu.slgx"));
    assert_eq!(app.library, "cpu");

    // Saved somewhere else, or the file renamed: the library name is
    // what other projects refer to these circuits by, so it must not
    // move underneath them.
    app.name_library_after(std::path::Path::new("/tmp/cpu-backup.slgx"));
    assert_eq!(app.library, "cpu");
}

#[test]
fn the_library_name_survives_undo_of_a_later_edit() {
    let mut app = SimLogixApp::default();
    app.name_library_after(std::path::Path::new("/tmp/cpu.slgx"));
    app.create_circuit(String::new());

    app.undo();

    assert_eq!(app.library, "cpu");
}

#[test]
fn renaming_the_project_is_undoable_and_refuses_an_empty_name() {
    let mut app = SimLogixApp::default();
    app.name_library_after(std::path::Path::new("/tmp/cpu.slgx"));

    app.rename_project("   ");
    assert_eq!(app.library, "cpu", "an empty name is not a name");

    app.rename_project("alu");
    assert_eq!(app.library, "alu");
    app.undo();
    assert_eq!(app.library, "cpu");
}

#[test]
fn the_simulation_view_takes_the_editing_tools_away() {
    let mut app = SimLogixApp {
        tool: Tool::Wire,
        ..Default::default()
    };

    app.switch_view(toolbar::View::Simulation);

    // A tool that places or wires would be a gesture the mode has just
    // promised not to have, left armed from before the switch.
    assert_eq!(app.tool, Tool::Select);
    assert!(app.selection.is_empty());
    // The band is a schematic gesture too; only the hand gives the
    // primary button back to the view here.
    assert!(!app.bands_on_left_drag());
    assert!(!app.pans_on_left_drag());
    app.sim_tool = toolbar::SimTool::Pan;
    assert!(app.pans_on_left_drag());
}

#[test]
fn a_left_drag_does_nothing_schematic_while_a_symbol_is_being_drawn() {
    let mut app = SimLogixApp {
        tool: Tool::Marquee,
        ..Default::default()
    };
    assert!(app.bands_on_left_drag());

    app.switch_view(toolbar::View::Appearance);

    // Both false, whatever the tool or the preference was: the primary
    // button belongs to the drawing here, and the middle one still pans.
    // The band is started from the scene's background response, outside
    // the appearance view's own code, so gating it there is what stops a
    // selection rectangle being swept under every line traced.
    assert!(!app.bands_on_left_drag());
    assert!(!app.pans_on_left_drag());

    app.left_drag_pans = true;
    assert!(!app.pans_on_left_drag());
}

#[test]
fn each_view_keeps_its_own_camera() {
    let mut app = SimLogixApp::default();
    let schematic = egui::Rect::from_min_max(egui::pos2(100.0, 100.0), egui::pos2(400.0, 300.0));
    app.scene_rect = schematic;

    app.switch_view(toolbar::View::Appearance);
    // A symbol always sits on the origin while a schematic sits wherever
    // it was drawn, so carrying one camera across would land nowhere.
    assert_ne!(app.scene_rect, schematic);

    app.switch_view(toolbar::View::Schematic);
    assert_eq!(app.scene_rect, schematic, "and it comes back untouched");
}

#[test]
fn the_appearance_view_frames_the_symbol_rather_than_the_drawing() {
    let mut app = SimLogixApp::default();
    app.place(ComponentKind::InputPort, egui::pos2(600.0, 600.0));
    app.switch_view(toolbar::View::Appearance);

    let framed = app.content_rect().expect("a symbol always has an extent");
    // Around the origin, where a symbol is drawn — not on the schematic
    // 600 away, which is what would put the symbol off screen. Not
    // exactly *on* the origin: a box with a pin down one side only is
    // not symmetric about it, and needn't be.
    assert!(framed.center().to_vec2().length() < canvas::BOX_SIZE.x);
    assert!(!framed.contains(egui::pos2(600.0, 600.0)));
}

#[test]
fn a_circuit_with_a_symbol_of_its_own_stops_showing_the_generated_box() {
    let mut app = SimLogixApp::default();
    app.place(ComponentKind::InputPort, egui::pos2(40.0, 40.0));
    app.rename_circuit(0, "sub");
    // A symbol reaching further than the generated box would, so the
    // instance's own rect has to come from the symbol rather than from
    // the port count.
    app.circuits[0].appearance = Some(crate::appearance::Appearance {
        shapes: vec![crate::appearance::Shape::Circle {
            center: (0.0, 0.0),
            radius: 90.0,
        }],
        pins: vec![crate::appearance::PinSlot {
            at: (-90.0, 0.0),
            facing: crate::appearance::Facing::Left,
            lead: 10.0,
            show_name: true,
        }],
        show_name: true,
    });

    app.create_circuit(String::new());
    let instance = app.place(
        ComponentKind::Circuit("sub".to_string()),
        egui::pos2(200.0, 200.0),
    );

    let placed = app
        .placed
        .iter()
        .find(|placed| placed.id() == instance)
        .expect("just placed");
    assert_eq!(placed.rect().width(), 180.0, "the symbol decides its size");
}

/// Builds `outer` containing an instance of `inner`, each with one input
/// port and one output port, and returns the app with `main` open and an
/// instance of `outer` placed in it.
///
/// Two levels deep on purpose: one level has worked since instances
/// existed, and that is exactly what hid this.
fn nested_inverters(depth: usize) -> (SimLogixApp, ComponentId, ComponentId) {
    let mut app = SimLogixApp::default();

    // The innermost circuit: in → NOT → out.
    app.rename_circuit(0, "level0");
    let input = app.place(ComponentKind::InputPort, egui::pos2(0.0, 0.0));
    let gate = app.place(ComponentKind::Not, egui::pos2(80.0, 0.0));
    let output = app.place(ComponentKind::OutputPort, egui::pos2(160.0, 0.0));
    app.add_wire(
        WireEndpoint::Pin(input, 0),
        WireEndpoint::Pin(gate, 0),
        Vec::new(),
    );
    app.add_wire(
        WireEndpoint::Pin(gate, 1),
        WireEndpoint::Pin(output, 0),
        Vec::new(),
    );

    // Each further level wraps the one below in ports of its own.
    for level in 1..depth {
        app.create_circuit(String::new());
        app.rename_circuit(app.active, &format!("level{level}"));
        let input = app.place(ComponentKind::InputPort, egui::pos2(0.0, 0.0));
        let inside = app.place(
            ComponentKind::Circuit(format!("level{}", level - 1)),
            egui::pos2(80.0, 0.0),
        );
        let output = app.place(ComponentKind::OutputPort, egui::pos2(160.0, 0.0));
        app.add_wire(
            WireEndpoint::Pin(input, 0),
            WireEndpoint::Pin(inside, 0),
            Vec::new(),
        );
        app.add_wire(
            WireEndpoint::Pin(inside, 1),
            WireEndpoint::Pin(output, 0),
            Vec::new(),
        );
    }

    // And a circuit that drives the outermost one from a switch.
    app.create_circuit(String::new());
    let switch = app.place(ComponentKind::Switch, egui::pos2(0.0, 200.0));
    let instance = app.place(
        ComponentKind::Circuit(format!("level{}", depth - 1)),
        egui::pos2(120.0, 200.0),
    );
    app.add_wire(
        WireEndpoint::Pin(switch, 0),
        WireEndpoint::Pin(instance, 0),
        Vec::new(),
    );
    app.rebuild_nets();
    app.advance_circuit(SETTLE_TICKS);
    (app, switch, instance)
}

#[test]
fn one_level_of_nesting_inverts() {
    let (app, _, instance) = nested_inverters(1);
    let out = app.circuit.pins(instance)[1].net;
    // The switch rests off, so the inverter's output is high.
    assert_eq!(app.circuit.signal_at(out), simlogix_core::Signal::High);
}

#[test]
fn a_sub_circuit_inside_a_sub_circuit_stays_connected() {
    // Romain's `and` is a `nand` followed by a `not`, so its instances
    // are two levels deep — and at two levels the inner circuit's own
    // wiring went missing, leaving the output driven by nobody.
    let (app, _, instance) = nested_inverters(2);
    let out = app.circuit.pins(instance)[1].net;
    assert_eq!(app.circuit.signal_at(out), simlogix_core::Signal::High);
}

#[test]
fn nesting_has_no_depth_limit() {
    // Eight levels, which is well past anything that could be special
    // cased. Each level only wraps the one below in ports of its own, so
    // there is still exactly one inverter at the bottom however deep it
    // is buried — and that is the point: the depth must cost nothing but
    // depth.
    let (mut app, switch, instance) = nested_inverters(8);
    let out = app.circuit.pins(instance)[1].net;
    assert_eq!(app.circuit.signal_at(out), simlogix_core::Signal::High);

    // Driven, not merely connected: a net can be `High` because
    // something reaches it, and the only proof it is the *input* that
    // reaches it is that changing the input changes it.
    let flipped = Properties {
        pressed: Some(true),
        ..Default::default()
    };
    if let Some(placed) = app.placed.iter_mut().find(|p| p.id() == switch) {
        placed.set_properties(flipped);
    }
    app.circuit.schedule_now(switch);
    app.advance_circuit(SETTLE_TICKS);

    assert_eq!(app.circuit.signal_at(out), simlogix_core::Signal::Low);
}

#[test]
fn a_sub_circuit_joining_two_ports_and_nothing_else_still_passes_through() {
    let mut app = SimLogixApp::default();
    // The sub-circuit is a plain feed-through: an input port wired
    // straight to an output port, with no component in between. That
    // inner net has no pin of its own to hang the connection on, which
    // is what made this the one shape that silently went dead.
    let input = app.place(ComponentKind::InputPort, egui::pos2(40.0, 40.0));
    let output = app.place(ComponentKind::OutputPort, egui::pos2(120.0, 40.0));
    app.add_wire(
        WireEndpoint::Pin(input, 0),
        WireEndpoint::Pin(output, 0),
        Vec::new(),
    );
    app.rename_circuit(0, "buf");

    app.create_circuit(String::new());
    let instance = app.place(
        ComponentKind::Circuit("buf".to_string()),
        egui::pos2(200.0, 200.0),
    );
    app.rebuild_nets();

    let pins = app.circuit.pins(instance);
    assert_eq!(pins.len(), 2, "one pin per port");
    assert_eq!(
        pins[0].net, pins[1].net,
        "the two sides of a feed-through are one net, so whatever is \
             wired to one arrives at the other"
    );
}

/// A project holding a circuit named `sub`, and — open — a second one
/// containing an instance of it.
fn project_with_an_instance() -> SimLogixApp {
    let mut app = SimLogixApp::default();
    app.rename_circuit(0, "sub");
    app.create_circuit(String::new());
    app.place(
        ComponentKind::Circuit("sub".to_string()),
        egui::pos2(80.0, 80.0),
    );
    app
}

/// What the open circuit's single instance names.
fn instance_path(app: &SimLogixApp) -> ComponentKind {
    app.placed[0].kind()
}

#[test]
fn renaming_a_circuit_carries_the_instances_of_it_along() {
    let mut app = project_with_an_instance();

    app.rename_circuit(0, "adder");

    // A reference is a path, so without repointing this instance would
    // still name a circuit that no longer exists — silently, since
    // nothing about the drawing changes.
    assert_eq!(
        instance_path(&app),
        ComponentKind::Circuit("adder".to_string())
    );
}

#[test]
fn re_filing_a_circuit_carries_the_instances_of_it_along() {
    let mut app = project_with_an_instance();
    app.create_folder("");
    let folder = app.folders[0].clone();

    app.move_circuit(0, folder.clone());

    // The folder is part of the address, so moving changes the path
    // exactly as a rename does.
    assert_eq!(
        instance_path(&app),
        ComponentKind::Circuit(format!("{folder}/sub"))
    );
}

#[test]
fn renaming_a_folder_carries_instances_of_everything_inside_it() {
    let mut app = project_with_an_instance();
    app.folders = vec!["alu".to_string()];
    app.circuits[0].folder = "alu".to_string();
    // The instance was placed before the move, so point it at where the
    // circuit is now, as a real session would have.
    app.repoint_instances("sub", "alu/sub");

    app.rename_folder("alu", "arith");

    // Nothing about this instance's own circuit was touched — it is
    // carried by a rename two levels away from it.
    assert_eq!(
        instance_path(&app),
        ComponentKind::Circuit("arith/sub".to_string())
    );
}

#[test]
fn an_instance_in_a_circuit_that_is_not_open_is_repointed_too() {
    let mut app = project_with_an_instance();
    // Close the circuit holding the instance by opening the other one.
    app.switch_to(0);

    app.rename_circuit(0, "adder");

    assert_eq!(
        app.circuits[1].components[0].kind,
        ComponentKind::Circuit("adder".to_string()),
        "a closed circuit is only in its saved form, and is just as \
             capable of holding a reference"
    );
}

#[test]
fn deleting_a_circuit_something_still_uses_is_refused() {
    let mut app = project_with_an_instance();

    app.delete_circuit(0);

    // Unlike a rename there is no new path to offer, so the only
    // alternative to refusing is leaving the instance naming nothing.
    assert_eq!(app.circuits.len(), 2);
    assert!(app.error.is_some());
}

#[test]
fn deleting_a_circuit_nothing_uses_is_allowed() {
    let mut app = project_with_an_instance();
    app.create_circuit(String::new());
    let spare = app.circuits.len() - 1;

    app.delete_circuit(spare);

    assert_eq!(app.circuits.len(), 2);
    assert!(app.error.is_none());
}

#[test]
fn renaming_a_folder_carries_everything_filed_under_it() {
    let mut app = SimLogixApp {
        folders: vec![
            "alu".to_string(),
            "alu/decode".to_string(),
            // Shares a prefix with "alu" but is not inside it. This is
            // the one a naive `starts_with` gets wrong.
            "alu2".to_string(),
        ],
        ..Default::default()
    };
    app.circuits[0].folder = "alu/decode".to_string();

    app.rename_folder("alu", "arith");

    assert_eq!(
        app.folders,
        vec!["arith", "arith/decode", "alu2"],
        "only what was inside should move"
    );
    assert_eq!(app.circuits[0].folder, "arith/decode");
}

#[test]
fn deleting_a_folder_lifts_its_contents_rather_than_taking_them_along() {
    let mut app = SimLogixApp {
        folders: vec!["alu".to_string(), "alu/decode".to_string()],
        ..Default::default()
    };
    app.circuits[0].folder = "alu/decode".to_string();
    app.create_circuit("alu".to_string());
    let lifted = app.circuits.len() - 1;

    app.delete_folder("alu");

    assert_eq!(app.folders, vec!["decode"]);
    // Filing something away is a presentation choice; undoing it must
    // not be able to take circuits with it.
    assert_eq!(app.circuits[0].folder, "decode");
    assert_eq!(app.circuits[lifted].folder, "");
}

#[test]
fn a_new_folder_never_takes_a_path_already_in_use() {
    let mut app = SimLogixApp::default();
    app.create_folder("");
    app.create_folder("");
    app.create_folder("");

    let distinct: std::collections::HashSet<&String> = app.folders.iter().collect();
    assert_eq!(distinct.len(), 3, "got {:?}", app.folders);
}

#[test]
fn moving_a_circuit_changes_where_it_is_filed_but_not_its_name() {
    let mut app = SimLogixApp::default();
    app.create_folder("");
    let folder = app.folders[0].clone();
    let name = app.circuits[0].name.clone();

    app.move_circuit(0, folder.clone());

    assert_eq!(app.circuits[0].folder, folder);
    // The whole point of folders being presentation: a reference to this
    // circuit is `library:name`, and filing it hasn't touched that.
    assert_eq!(app.circuits[0].name, name);

    app.undo();
    assert_eq!(app.circuits[0].folder, "");
}

#[test]
fn a_folder_rename_refuses_a_path_separator() {
    let mut app = SimLogixApp {
        folders: vec!["alu".to_string()],
        ..Default::default()
    };

    // This would move the folder rather than rename it, which isn't
    // what the gesture says it does.
    app.rename_folder("alu", "fpu/inner");

    assert_eq!(app.folders, vec!["alu"]);
}

#[test]
fn two_folders_can_each_hold_a_circuit_of_the_same_name() {
    // What choosing `library:folder/name` as the reference buys: the
    // folder is part of what identifies a circuit, so the name only has
    // to be distinct within it.
    let mut app = SimLogixApp {
        folders: vec!["alu".to_string(), "fpu".to_string()],
        ..Default::default()
    };
    app.create_circuit("alu".to_string());
    app.rename_circuit(app.active, "adder");
    app.create_circuit("fpu".to_string());
    app.rename_circuit(app.active, "adder");

    let filed: Vec<(&str, &str)> = app
        .circuits
        .iter()
        .map(|circuit| (circuit.folder.as_str(), circuit.name.as_str()))
        .collect();
    assert!(filed.contains(&("alu", "adder")), "got {filed:?}");
    assert!(filed.contains(&("fpu", "adder")), "got {filed:?}");
}

#[test]
fn moving_onto_a_name_already_in_that_folder_is_refused() {
    let mut app = SimLogixApp {
        folders: vec!["alu".to_string()],
        ..Default::default()
    };
    app.rename_circuit(0, "adder");
    app.move_circuit(0, "alu".to_string());
    app.create_circuit(String::new());
    let second = app.active;
    app.rename_circuit(second, "adder");

    app.move_circuit(second, "alu".to_string());

    assert_eq!(
        app.circuits[second].folder, "",
        "the move should not happen"
    );
    assert!(app.error.is_some(), "the clash should be reported");
}

#[test]
fn lifting_two_circuits_of_the_same_name_into_one_folder_renames_the_second() {
    // Deleting a folder must not be blocked by a name collision, so the
    // one that lands second gets a free name instead.
    let mut app = SimLogixApp {
        folders: vec!["alu".to_string()],
        ..Default::default()
    };
    app.rename_circuit(0, "adder");
    app.create_circuit("alu".to_string());
    let filed = app.active;
    app.rename_circuit(filed, "adder");

    app.delete_folder("alu");

    assert!(app.folders.is_empty());
    let names: Vec<&str> = app
        .circuits
        .iter()
        .map(|circuit| circuit.name.as_str())
        .collect();
    let distinct: std::collections::HashSet<&&str> = names.iter().collect();
    assert_eq!(distinct.len(), names.len(), "got {names:?}");
}

#[test]
fn properties_survive_a_save_and_load() {
    let mut app = SimLogixApp::default();
    let id = app.place(ComponentKind::Led, egui::pos2(40.0, 40.0));
    if let Some(placed) = app.placed.iter_mut().find(|placed| placed.id() == id) {
        placed.set_properties(Properties {
            name: Some("status".to_string()),
            color: Some([0, 200, 0]),
            ..Default::default()
        });
    }

    let project = app.to_project();
    let reloaded = SimLogixApp::from_project(&project, 0);

    let properties = reloaded.placed[0].properties();
    assert_eq!(properties.label(), Some("status"));
    assert_eq!(properties.color, Some([0, 200, 0]));
}

#[test]
fn a_component_with_nothing_set_writes_no_properties_at_all() {
    let mut app = SimLogixApp::default();
    app.place(ComponentKind::Led, egui::pos2(40.0, 40.0));

    let json = serde_json::to_string(&app.to_project()).expect("serializes");

    // The whole point of every property being optional: a project that
    // never touched them looks exactly as it did before they existed.
    assert!(!json.contains("properties"), "got {json}");
}

/// Two wires end to end, so they share a net: colouring one has to
/// colour both, because a net is one conductor.
fn two_wires_one_net() -> (SimLogixApp, u64, u64) {
    let mut app = SimLogixApp::default();
    let button = app.place(ComponentKind::Button, egui::pos2(0.0, 0.0));
    let led = app.place(ComponentKind::Led, egui::pos2(200.0, 0.0));

    let first = app.add_wire(
        WireEndpoint::Pin(button, 0),
        WireEndpoint::Junction {
            wire: 0,
            waypoint: 0,
        },
        vec![egui::pos2(100.0, 0.0)],
    );
    let second = app.add_wire(
        WireEndpoint::Junction {
            wire: first,
            waypoint: 0,
        },
        WireEndpoint::Pin(led, 0),
        Vec::new(),
    );
    // The first wire's junction was a placeholder until the second one
    // existed; point it at a real host now.
    if let Some(wire) = app.wires.iter_mut().find(|wire| wire.id == first) {
        wire.from = WireEndpoint::Pin(button, 0);
    }
    app.rebuild_nets();
    (app, first, second)
}

#[test]
fn colouring_one_wire_colours_the_whole_net() {
    let (mut app, first, second) = two_wires_one_net();

    app.color_net(first, Some([10, 20, 30]));

    let color_of = |app: &SimLogixApp, id: u64| {
        app.wires
            .iter()
            .find(|wire| wire.id == id)
            .and_then(|wire| wire.color)
    };
    assert_eq!(color_of(&app, first), Some([10, 20, 30]));
    assert_eq!(
        color_of(&app, second),
        Some([10, 20, 30]),
        "the net is one conductor, so it is one colour"
    );
}

#[test]
fn a_wire_joining_a_coloured_net_inherits_its_colour() {
    let (mut app, first, second) = two_wires_one_net();
    app.color_net(first, Some([10, 20, 30]));

    // A fresh wire onto the same net -- it starts with no colour of its
    // own, and picking it up is what keeps "the net is coloured" true.
    let added = app.add_wire(
        WireEndpoint::Junction {
            wire: second,
            waypoint: 0,
        },
        WireEndpoint::Free(egui::pos2(300.0, 40.0)),
        Vec::new(),
    );
    app.rebuild_nets();

    let added_color = app
        .wires
        .iter()
        .find(|wire| wire.id == added)
        .and_then(|wire| wire.color);
    assert_eq!(added_color, Some([10, 20, 30]));
}

#[test]
fn joining_two_differently_coloured_nets_keeps_both_colours() {
    let (mut app, first, second) = two_wires_one_net();
    // Force a disagreement, which is what happens when two coloured nets
    // are joined: no winner is picked, because a silent choice is worse
    // than a visibly two-tone net the user can re-colour.
    if let Some(wire) = app.wires.iter_mut().find(|wire| wire.id == first) {
        wire.color = Some([1, 1, 1]);
    }
    if let Some(wire) = app.wires.iter_mut().find(|wire| wire.id == second) {
        wire.color = Some([2, 2, 2]);
    }

    app.rebuild_nets();

    let colors: Vec<Option<[u8; 3]>> = app.wires.iter().map(|wire| wire.color).collect();
    assert!(colors.contains(&Some([1, 1, 1])), "got {colors:?}");
    assert!(colors.contains(&Some([2, 2, 2])), "got {colors:?}");
}

#[test]
fn a_wire_colour_survives_a_save_and_load() {
    let (mut app, first, _) = two_wires_one_net();
    app.color_net(first, Some([7, 8, 9]));

    let project = app.to_project();
    let reloaded = SimLogixApp::from_project(&project, 0);

    assert!(reloaded
        .wires
        .iter()
        .all(|wire| wire.color == Some([7, 8, 9])));
}

#[test]
fn changing_a_component_to_its_sibling_kind_keeps_its_wires_and_place() {
    let mut app = SimLogixApp::default();
    let transistor = app.place(ComponentKind::NTransistor, egui::pos2(60.0, 60.0));
    let led = app.place(ComponentKind::Led, egui::pos2(200.0, 60.0));
    app.add_wire(
        WireEndpoint::Pin(transistor, 2),
        WireEndpoint::Pin(led, 0),
        vec![egui::pos2(140.0, 60.0)],
    );

    app.change_kind(transistor, ComponentKind::PTransistor);

    // The rebuild hands out fresh ids, so what's checked is that the
    // drawing survived: same components in the same places, same wire,
    // same route, and the selection still on the thing being edited.
    assert_eq!(app.placed.len(), 2);
    assert_eq!(app.placed[0].kind(), ComponentKind::PTransistor);
    assert_eq!(app.placed[0].center(), egui::pos2(60.0, 60.0));
    assert_eq!(app.wires.len(), 1);
    assert_eq!(app.wires[0].waypoints, vec![egui::pos2(140.0, 60.0)]);
    assert_eq!(app.selection.lone_component(), Some(app.placed[0].id()));
}

#[test]
fn changing_a_kind_is_undoable() {
    let mut app = SimLogixApp::default();
    let id = app.place(ComponentKind::BusTransceiver, egui::pos2(60.0, 60.0));

    app.record_edit();
    app.change_kind(id, ComponentKind::BusTransceiverOe);
    assert_eq!(app.placed[0].kind(), ComponentKind::BusTransceiverOe);

    app.undo();
    assert_eq!(app.placed[0].kind(), ComponentKind::BusTransceiver);
}

#[test]
fn copying_a_selection_and_pasting_it_duplicates_the_wire_between_them() {
    let mut app = SimLogixApp::default();
    let button = app.place(ComponentKind::Button, egui::pos2(40.0, 40.0));
    let led = app.place(ComponentKind::Led, egui::pos2(160.0, 40.0));
    let wire = app.add_wire(
        WireEndpoint::Pin(button, 0),
        WireEndpoint::Pin(led, 0),
        vec![egui::pos2(100.0, 40.0)],
    );
    app.selection.components.insert(button);
    app.selection.components.insert(led);
    app.selection.wires.insert(wire);

    let fragment = app.copied_fragment().expect("something is selected");
    app.paste_fragment(&fragment);

    assert_eq!(app.placed.len(), 4);
    assert_eq!(app.wires.len(), 2);
    // The copy lands offset, and is what's selected afterwards, so a
    // drag or a second paste acts on it rather than the original.
    assert_eq!(app.selection.components.len(), 2);
    assert_eq!(app.selection.wires.len(), 1);
    assert!(app
        .placed
        .iter()
        .any(|placed| placed.center() == egui::pos2(60.0, 60.0)));
}

#[test]
fn a_wire_with_one_end_outside_the_selection_is_not_copied() {
    let mut app = SimLogixApp::default();
    let button = app.place(ComponentKind::Button, egui::pos2(40.0, 40.0));
    let led = app.place(ComponentKind::Led, egui::pos2(160.0, 40.0));
    let wire = app.add_wire(
        WireEndpoint::Pin(button, 0),
        WireEndpoint::Pin(led, 0),
        Vec::new(),
    );
    // Only one end's component is taken along, so the wire has nowhere
    // to attach — copying it anyway would paste an end you never chose.
    app.selection.components.insert(button);
    app.selection.wires.insert(wire);

    let fragment = app.copied_fragment().expect("something is selected");
    app.paste_fragment(&fragment);

    assert_eq!(app.placed.len(), 3, "the button alone should be pasted");
    assert_eq!(app.wires.len(), 1, "the original wire, and no copy");
}

#[test]
fn pasting_something_that_is_not_a_fragment_does_nothing() {
    let mut app = SimLogixApp::default();
    app.place(ComponentKind::Led, egui::pos2(40.0, 40.0));

    // The system clipboard usually holds someone else's text. Pasting it
    // into the canvas has to be a no-op, not a half-read.
    app.paste_fragment("https://example.com");
    app.paste_fragment("{\"components\": []}");
    app.paste_fragment("");

    assert_eq!(app.placed.len(), 1);
}

#[test]
fn settings_written_by_an_older_build_still_load() {
    // Every field optional or defaulted, so a settings file missing the
    // ones a later build added still reads — the same rule the project
    // format follows, and the reason a preference can be added without
    // resetting everyone's others.
    let settings: Settings = ron::from_str("()").expect("parses");
    assert_eq!(settings.language, None);
    assert!(!settings.left_drag_pans);
    assert!(settings.recent.is_empty());
}

#[test]
fn a_chosen_language_survives_a_round_trip() {
    let stored = Settings {
        language: Some(Language::French),
        left_drag_pans: true,
        recent: vec![PathBuf::from("/tmp/alu.slgx")],
    };

    let text = ron::to_string(&stored).expect("serializes");
    let read: Settings = ron::from_str(&text).expect("parses");

    // `None` has to stay distinguishable from a chosen language: it is
    // what keeps following the OS locale.
    assert_eq!(read.language, Some(Language::French));
    assert!(read.left_drag_pans);
    assert_eq!(read.recent, stored.recent);
}

#[test]
fn reopening_a_project_moves_it_up_the_recent_list_rather_than_repeating_it() {
    let mut app = SimLogixApp::default();
    for name in ["a.slgx", "b.slgx", "c.slgx"] {
        app.remember_recent(&PathBuf::from(format!("/tmp/{name}")));
    }
    assert_eq!(app.recent.len(), 3);

    app.remember_recent(&PathBuf::from("/tmp/c.slgx"));

    assert_eq!(app.recent.len(), 3, "no duplicate entry");
    assert_eq!(
        app.recent[0],
        PathBuf::from("/tmp/c.slgx"),
        "and it is first"
    );
}

#[test]
fn the_recent_list_stops_at_its_limit() {
    let mut app = SimLogixApp::default();
    for index in 0..MAX_RECENT + 4 {
        app.remember_recent(&PathBuf::from(format!("/tmp/{index}.slgx")));
    }

    assert_eq!(app.recent.len(), MAX_RECENT);
    // The newest is kept and the oldest dropped, not the other way round.
    assert_eq!(
        app.recent[0],
        PathBuf::from(format!("/tmp/{}.slgx", MAX_RECENT + 3))
    );
}

#[test]
fn a_project_that_will_not_open_is_dropped_from_the_list() {
    let mut app = SimLogixApp::default();
    let missing = PathBuf::from("/tmp/simlogix-does-not-exist.slgx");
    app.remember_recent(&missing);

    app.open_path(missing.clone());

    // Failing to read *is* the answer to "is this still here?", so there
    // is no separate existence check to keep in step with it.
    assert!(!app.recent.contains(&missing));
    assert!(app.error.is_some());
}

#[test]
fn pasting_is_undoable() {
    let mut app = SimLogixApp::default();
    let id = app.place(ComponentKind::Led, egui::pos2(40.0, 40.0));
    app.selection.components.insert(id);
    let fragment = app.copied_fragment().expect("something is selected");

    app.paste_fragment(&fragment);
    assert_eq!(app.placed.len(), 2);

    app.undo();
    assert_eq!(app.placed.len(), 1);
}

#[test]
fn a_switchs_position_is_saved_where_a_buttons_press_is_not() {
    let mut app = SimLogixApp::default();
    let switch = app.place(ComponentKind::Switch, egui::pos2(40.0, 40.0));
    if let Some(placed) = app.placed.iter_mut().find(|p| p.id() == switch) {
        let mut properties = placed.properties().clone();
        properties.pressed = Some(true);
        placed.set_properties(properties);
    }

    let project = app.to_project();
    let reloaded = SimLogixApp::from_project(&project, 0);

    // The line the document draws: what the user set is kept, what the
    // simulation produced is not. A latched switch is the former.
    assert_eq!(reloaded.placed[0].properties().pressed, Some(true));
}

#[test]
fn the_framed_area_covers_what_is_actually_drawn() {
    let mut app = SimLogixApp::default();
    // Far from the origin, which is exactly the case that used to open
    // outside the visible area.
    app.place(ComponentKind::Led, egui::pos2(2000.0, 3000.0));

    let content = app.content_rect().expect("something is placed");

    assert!(content.contains(egui::pos2(2000.0, 3000.0)));
    assert!(!content.contains(egui::Pos2::ZERO));
}

#[test]
fn a_loose_wire_end_is_framed_too() {
    let mut app = SimLogixApp::default();
    let led = app.place(ComponentKind::Led, egui::pos2(0.0, 0.0));
    app.add_wire(
        WireEndpoint::Pin(led, 0),
        // Nothing anchors this to a component, so only the wire knows
        // the drawing reaches out there.
        WireEndpoint::Free(egui::pos2(900.0, 0.0)),
        vec![egui::pos2(400.0, 0.0)],
    );

    let content = app.content_rect().expect("something is placed");

    assert!(content.contains(egui::pos2(900.0, 0.0)));
}

#[test]
fn an_empty_circuit_has_nothing_to_frame() {
    assert!(SimLogixApp::default().content_rect().is_none());
}

#[test]
fn switching_circuits_asks_for_a_refit_but_undo_does_not() {
    let mut app = SimLogixApp::default();
    app.create_circuit(String::new());
    app.switch_to(0);
    assert!(app.refit_view, "the other circuit may be drawn anywhere");

    app.refit_view = false;
    app.undo();
    // Stepping back through your own edits must not move the view.
    assert!(!app.refit_view);
}

#[test]
fn saving_carries_every_circuit_not_just_the_open_one() {
    let mut app = SimLogixApp::default();
    app.place(ComponentKind::Button, egui::pos2(40.0, 40.0));
    app.create_circuit(String::new());
    app.place(ComponentKind::Led, egui::pos2(80.0, 80.0));

    let project = app.to_project();

    assert_eq!(project.circuits.len(), 2);
    assert_eq!(project.circuits[0].components.len(), 1);
    assert_eq!(project.circuits[1].components.len(), 1);
}

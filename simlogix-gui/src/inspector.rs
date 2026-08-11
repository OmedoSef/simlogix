//! The circuit as the engine sees it.
//!
//! Every bug found in this project so far was found by writing a throwaway
//! test that printed exactly this and reading it: a clock that never moved,
//! a CMOS NAND that read `Error`, a two-bit port that showed `E`. Each time
//! the answer was one line of engine state that nothing in the application
//! could show.
//!
//! The line that matters is a net's **contributions**. That a net resolved
//! to `Error` says nothing; that one port is driving one bit onto a net of
//! two says everything.
//!
//! It reads and never writes — a view, not an editor — so it can be left
//! open while you work.

use egui::{Context, RichText, Ui};
use simlogix_core::{Circuit, ComponentId, NetId, Signal};

use crate::i18n::Strings;

/// One row's worth of what a component is, for a reader.
pub struct Named {
    pub id: ComponentId,
    /// The user's own name if they gave one, else the kind's label.
    pub label: String,
    /// How wide each of its pins is, in order. The engine does not know
    /// this — a net's width is derived *from* it — so it has to be handed
    /// over, and it is the number that makes a disagreement visible.
    ///
    /// Per *pin*, because a component's pins are not always alike: reporting
    /// a splitter's bus width against a branch net would invent a mismatch
    /// where there is none, in the one window whose whole job is to say
    /// where a real one is.
    ///
    /// `None` is a pin that declares nothing and takes whatever its net
    /// carries — a `Probe`. It is shown at the net's own width and can
    /// never be the thing that disagrees.
    pub pin_widths: Vec<Option<usize>>,
}

/// The whole of what this window shows, as text to paste into a bug report.
///
/// A report that says "it does not work" costs a round trip to answer; this
/// is the answer to the first three questions — which build, on what, and
/// what the engine actually thinks — in a form that can be pasted rather
/// than described. The `.slgx` still helps most, and the issue template
/// asks for both.
///
/// Deliberately not the project itself: a circuit is the user's, and a
/// window with a *copy* button should not quietly hand over the drawing.
pub fn report(strings: &Strings, circuit: &Circuit, named: &[Named]) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "SimLogix {}{}  ·  {}  ·  tick {}\n",
        env!("CARGO_PKG_VERSION"),
        crate::app::BUILD_MARKER,
        std::env::consts::OS,
        circuit.now(),
    ));
    match circuit.next_event_tick() {
        Some(tick) => out.push_str(&format!("next event at tick {tick}\n")),
        None => out.push_str("nothing scheduled\n"),
    }
    out.push('\n');

    for net in circuit.nets() {
        let signal = circuit.signal_at(net);
        out.push_str(&format!(
            "net {} · {} bits · {}{}\n",
            net.0,
            circuit.net_width(net),
            describe(&signal),
            if circuit.is_weakly_driven(net) {
                " · weak"
            } else {
                ""
            },
        ));
        let contributions = circuit.contributions(net);
        if contributions.is_empty() {
            out.push_str("    (undriven)\n");
        }
        for ((component, index), signal) in contributions {
            out.push_str(&format!(
                "    {} · pin {} · {} bits · {}\n",
                label_of(named, component),
                index,
                signal.width(),
                describe(&signal),
            ));
        }
        for (component, index) in circuit.readers(net) {
            let declared = width_of(named, component, index);
            out.push_str(&format!(
                "    {} · pin {} · {} bits · reads{}\n",
                label_of(named, component),
                index,
                declared.unwrap_or_else(|| circuit.net_width(net)),
                if declared.is_none_or(|width| width == circuit.net_width(net)) {
                    ""
                } else {
                    "  (width mismatch)"
                },
            ));
        }
    }
    let _ = strings;
    out
}

/// Draws the window while `open`, and lets its close button clear the flag.
///
/// `focus` narrows it to the nets those components touch — the selection,
/// in practice. Everything at once is unreadable past a few dozen nets, and
/// what you want is nearly always "this thing, and what it is arguing
/// with".
pub fn show(
    ctx: &Context,
    strings: &Strings,
    circuit: &Circuit,
    named: &[Named],
    focus: &[ComponentId],
    open: &mut bool,
) {
    egui::Window::new(strings.inspector_title)
        .open(open)
        .collapsible(false)
        .resizable(true)
        .default_width(520.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(
                        strings
                            .status_tick
                            .replace("{}", &circuit.now().to_string()),
                    )
                    .strong(),
                );
                ui.separator();
                match circuit.next_event_tick() {
                    Some(tick) => ui.label(
                        strings
                            .inspector_next_event
                            .replace("{}", &tick.to_string()),
                    ),
                    None => ui.label(RichText::new(strings.inspector_settled).weak()),
                };
            });
            if ui
                .button(strings.inspector_copy)
                .on_hover_text(strings.inspector_copy_hint)
                .clicked()
            {
                ui.ctx().copy_text(report(strings, circuit, named));
            }
            ui.separator();

            let nets = nets_to_show(circuit, focus);
            if nets.is_empty() {
                ui.label(RichText::new(strings.inspector_nothing).weak());
                return;
            }
            if !focus.is_empty() {
                ui.label(RichText::new(strings.inspector_narrowed).weak());
                ui.add_space(4.0);
            }

            egui::ScrollArea::vertical()
                .max_height(420.0)
                .show(ui, |ui| {
                    for net in nets {
                        net_row(ui, strings, circuit, named, net);
                    }
                });
        });
}

/// The nets worth listing: those the focused components touch, or all of
/// them when nothing is focused.
fn nets_to_show(circuit: &Circuit, focus: &[ComponentId]) -> Vec<NetId> {
    if focus.is_empty() {
        return circuit.nets();
    }
    let mut nets: Vec<NetId> = focus
        .iter()
        .filter_map(|&component| circuit.try_pins(component))
        .flat_map(|pins| pins.iter().map(|pin| pin.net))
        .collect();
    nets.sort_by_key(|net| net.0);
    nets.dedup();
    nets
}

fn net_row(ui: &mut Ui, strings: &Strings, circuit: &Circuit, named: &[Named], net: NetId) {
    let signal = circuit.signal_at(net);
    let width = circuit.net_width(net);
    let heading = format!(
        "{}  ·  {}  ·  {}",
        strings.inspector_net.replace("{}", &net.0.to_string()),
        strings.inspector_bits.replace("{}", &width.to_string()),
        describe(&signal),
    );

    egui::CollapsingHeader::new(heading)
        .id_salt(("inspector_net", net.0))
        // Open by default: the contributions *are* what this window is for,
        // and a list of headings you have to unfold one by one to find the
        // one line you came for is the tool refusing to help.
        .default_open(true)
        .show(ui, |ui| {
            if circuit.is_weakly_driven(net) {
                ui.label(RichText::new(strings.inspector_weak).weak());
            }
            let contributions = circuit.contributions(net);
            if contributions.is_empty() {
                ui.label(RichText::new(strings.inspector_undriven).weak());
            }
            for ((component, index), signal) in contributions {
                // The width beside each contribution, not only the net's:
                // a mismatch is *only* visible as the two disagreeing, and
                // it is the commonest thing this window is opened for.
                ui.label(format!(
                    "{} · {} · {} · {}",
                    label_of(named, component),
                    strings.inspector_pin.replace("{}", &index.to_string()),
                    strings
                        .inspector_bits
                        .replace("{}", &signal.width().to_string()),
                    describe(&signal),
                ));
            }
            // The readers, which put nothing on the net and are therefore
            // invisible in everything above. A two-bit output reading a
            // four-bit bus is a real mistake that nothing else reports, and
            // this is the one line that shows it.
            for (component, index) in circuit.readers(net) {
                let declared = width_of(named, component, index);
                let mut row = format!(
                    "{} · {} · {} · {}",
                    label_of(named, component),
                    strings.inspector_pin.replace("{}", &index.to_string()),
                    strings
                        .inspector_bits
                        .replace("{}", &declared.unwrap_or(width).to_string()),
                    strings.inspector_reads,
                );
                if declared.is_some_and(|declared| declared != width) {
                    row.push_str(&format!("  ⚠ {}", strings.inspector_mismatch));
                }
                ui.label(row);
            }
        });
}

/// What a reading pin says it is, and whether that is a claim at all.
///
/// `None` means the pin declares nothing and takes the net's width, so it
/// is reported *at* that width and never marked — a probe is an instrument
/// reading a net, not something that can be wrong about it.
fn width_of(named: &[Named], component: ComponentId, index: usize) -> Option<usize> {
    named
        .iter()
        .find(|entry| entry.id == component)
        .and_then(|entry| entry.pin_widths.get(index))
        .copied()
        .unwrap_or(Some(1))
}

fn label_of(named: &[Named], component: ComponentId) -> &str {
    named
        .iter()
        .find(|entry| entry.id == component)
        .map(|entry| entry.label.as_str())
        // A component the drawing does not list is one flattened into the
        // circuit from a sub-circuit. Saying so is more use than a blank.
        .unwrap_or("(inner)")
}

/// Every level spelled out, rather than the one-character readout a probe
/// shows. This window is where you come *because* the short answer was not
/// enough.
fn describe(signal: &Signal) -> String {
    let bits: Vec<String> = signal
        .levels()
        .iter()
        .rev()
        .map(|level| format!("{level:?}"))
        .collect();
    bits.join(" ")
}

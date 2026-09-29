//! The window: an operation picker, its inputs, a Run button and the output. Operations run on
//! a worker thread so the window stays responsive during a flash.

use alloc::rc::Rc;
use std::io::{self, Write as _};
use std::process::ExitCode;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;

use eframe::egui;
use gsfw_core::ops::{Op, PadAction, Sink, run as run_op};

use form::{Form, Kind};

mod form;

/// What the worker thread sends back.
enum Msg {
    Line(String),
    Done(Result<bool, String>),
}

fn send(tx: &Sender<Msg>, ctx: &egui::Context, msg: Msg) {
    if tx.send(msg).is_ok() {
        ctx.request_repaint();
    }
}

fn spawn(op: Op, ctx: egui::Context) -> Receiver<Msg> {
    let (tx, rx) = channel();
    thread::spawn(move || {
        let lines = tx.clone();
        let repaint = ctx.clone();
        let sink: Sink =
            Rc::new(move |line: &str| send(&lines, &repaint, Msg::Line(line.to_owned())));
        let result = run_op(&op, &sink).map_err(|err| err.to_string());
        send(&tx, &ctx, Msg::Done(result));
    });
    rx
}

#[derive(Default)]
struct App {
    form: Form,
    output: Vec<String>,
    status: String,
    running: Option<Receiver<Msg>>,
    /// A flash that writes, waiting for the user's confirmation.
    confirm: Option<Op>,
}

/// Whether `op` writes to a pad's flash.
const fn writes(op: &Op) -> bool {
    matches!(
        op,
        Op::Pad {
            action: PadAction::Flash { write: true, .. },
            ..
        }
    )
}

/// The confirmation text for a flash that writes.
fn confirm_text(op: &Op) -> String {
    let Op::Pad { image, link, .. } = op else {
        return String::new();
    };
    let pad = link.pid.map_or_else(
        || "the first GameSir pad in GIP mode".to_owned(),
        |pid| format!("the pad with PID {pid:04x}"),
    );
    format!(
        "Write {} to {pad}?\n\nFlashing can make a pad unusable. Do not unplug the pad until the \
         output says \"done; replug the pad\".",
        image.display()
    )
}

impl App {
    fn poll(&mut self) {
        let Some(rx) = &self.running else { return };
        let mut done = None;
        while let Ok(msg) = rx.try_recv() {
            match msg {
                Msg::Line(line) => self.output.push(line),
                Msg::Done(result) => done = Some(result),
            }
        }
        if let Some(result) = done {
            self.running = None;
            self.status = match result {
                Ok(true) => "done".to_owned(),
                Ok(false) => "done; a check failed (see the output)".to_owned(),
                Err(err) => {
                    self.output.push(format!("error: {err}"));
                    "failed".to_owned()
                }
            };
        }
    }

    fn start(&mut self, ctx: &egui::Context) {
        match self.form.op() {
            Ok(op) if writes(&op) => self.confirm = Some(op),
            Ok(op) => self.launch(op, ctx),
            Err(err) => self.status = err,
        }
    }

    fn launch(&mut self, op: Op, ctx: &egui::Context) {
        self.output.clear();
        "running".clone_into(&mut self.status);
        self.running = Some(spawn(op, ctx.clone()));
    }

    /// The write confirmation dialog, while a flash waits for it.
    fn confirmation(&mut self, ctx: &egui::Context) {
        let Some(op) = self.confirm.take() else {
            return;
        };
        let mut answer = None;
        egui::Modal::new(egui::Id::new("confirm-write")).show(ctx, |ui| {
            ui.label(confirm_text(&op));
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    answer = Some(false);
                }
                if ui.button("Write").clicked() {
                    answer = Some(true);
                }
            });
        });
        match answer {
            Some(true) => self.launch(op, ctx),
            Some(false) => "cancelled; nothing written".clone_into(&mut self.status),
            None => self.confirm = Some(op),
        }
    }

    fn picker(&mut self, ui: &mut egui::Ui) {
        egui::ComboBox::from_id_salt("operation")
            .width(ui.available_width())
            .selected_text(self.form.kind.label())
            .show_ui(ui, |ui| {
                for kind in Kind::ALL {
                    ui.selectable_value(&mut self.form.kind, kind, kind.label());
                }
            });
    }

    fn paths(&mut self, ui: &mut egui::Ui) {
        let (first, second) = self.form.kind.paths();
        if let Some(label) = first {
            ui.label(label);
            ui.text_edit_singleline(&mut self.form.first);
        }
        if let Some(label) = second {
            ui.label(label);
            ui.text_edit_singleline(&mut self.form.second);
        }
        if self.form.kind == Kind::Fetch {
            ui.label("Downloaded file (optional; empty: the catalog URLs)");
            ui.text_edit_singleline(&mut self.form.from);
        }
        if self.form.kind == Kind::Crc {
            ui.horizontal(|ui| {
                ui.label("Address");
                ui.text_edit_singleline(&mut self.form.addr);
                ui.label("Length");
                ui.text_edit_singleline(&mut self.form.len);
            });
        }
    }

    fn pad_options(&mut self, ui: &mut egui::Ui) {
        if !self.form.kind.pad() {
            return;
        }
        if self.form.kind == Kind::Flash {
            ui.checkbox(
                &mut self.form.write,
                "Write flash (without it, only the checks run)",
            );
            ui.checkbox(
                &mut self.form.keep_region_a,
                "Keep region A (flash even though the device's region A differs)",
            );
        }
        egui::CollapsingHeader::new("Pad options").show(ui, |ui| {
            egui::Grid::new("pad-options")
                .num_columns(2)
                .show(ui, |ui| {
                    ui.label("USB PID (hex, empty: any)");
                    ui.text_edit_singleline(&mut self.form.link.pid);
                    ui.end_row();
                    ui.label("GIP header flags");
                    ui.text_edit_singleline(&mut self.form.link.flags);
                    ui.end_row();
                    ui.label("Session key (hex, empty: handshake)");
                    ui.text_edit_singleline(&mut self.form.link.session_key);
                    ui.end_row();
                });
            ui.checkbox(
                &mut self.form.link.power_on,
                "Send the xpad power-on message",
            );
            ui.checkbox(&mut self.form.link.verbose, "Show every USB message");
        });
    }

    fn controls(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let busy = self.running.is_some() || self.confirm.is_some();
            let text = if self.form.kind == Kind::Flash && self.form.write {
                "Flash"
            } else {
                "Run"
            };
            if ui.add_enabled(!busy, egui::Button::new(text)).clicked() {
                self.start(ui.ctx());
            }
            if ui.add_enabled(!busy, egui::Button::new("Clear")).clicked() {
                self.output.clear();
                self.status.clear();
            }
            ui.label(&self.status);
        });
    }

    fn dropped_file(&mut self, ui: &egui::Ui) {
        let dropped = ui.ctx().input(|input| input.raw.dropped_files.clone());
        if let Some(file) = dropped.first() {
            self.form.first = file.path().display().to_string();
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.poll();
        self.dropped_file(ui);
        self.confirmation(&ui.ctx().clone());
        egui::CentralPanel::default().show(ui, |ui| {
            self.picker(ui);
            self.paths(ui);
            self.pad_options(ui);
            self.controls(ui);
            ui.separator();
            egui::ScrollArea::both()
                .stick_to_bottom(true)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for line in &self.output {
                        ui.monospace(line);
                    }
                });
        });
    }
}

/// Opens the window.
#[must_use]
pub fn run() -> ExitCode {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("gsfw")
            .with_inner_size([760.0, 560.0])
            .with_drag_and_drop(true),
        ..eframe::NativeOptions::default()
    };
    let result = eframe::run_native("gsfw", options, Box::new(|_| Ok(Box::new(App::default()))));
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            if writeln!(io::stderr(), "gsfw-gui: {err}").is_err() {
                // Nowhere left to report it.
            }
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests;

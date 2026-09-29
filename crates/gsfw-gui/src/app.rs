//! The window: the operations in a side panel, and the chosen operation's fields and output.
//! Operations run on a worker thread so the window stays responsive during a flash.

use alloc::rc::Rc;
use std::io::{self, Write as _};
use std::process::ExitCode;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;

use eframe::egui;
use gsfw_core::catalog::{self, Artifact};
use gsfw_core::ops::{Op, PadAction, Sink, run as run_op};

use form::{Form, Group, Kind};

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

/// The state of the last operation, shown next to its button.
#[derive(Default)]
enum Status {
    #[default]
    Idle,
    Running,
    Done,
    CheckFailed,
    Failed,
    Cancelled,
    /// A field is empty or holds a bad number.
    Invalid(String),
}

impl Status {
    fn show(&self, ui: &mut egui::Ui) {
        let warn = ui.visuals().warn_fg_color;
        let error = ui.visuals().error_fg_color;
        match self {
            Self::Idle => {}
            Self::Running => {
                ui.spinner();
                ui.label("Working. Keep the controller connected.");
            }
            Self::Done => {
                ui.label("Done.");
            }
            Self::CheckFailed => {
                ui.colored_label(warn, "Done. A check failed. Read the output.");
            }
            Self::Failed => {
                ui.colored_label(error, "Failed. The last line of the output tells why.");
            }
            Self::Cancelled => {
                ui.weak("Cancelled. Nothing was written.");
            }
            Self::Invalid(err) => {
                ui.colored_label(error, err);
            }
        }
    }
}

struct App {
    form: Form,
    /// The built-in catalog, for the Fetch firmware picker.
    catalog: Result<Vec<Artifact>, String>,
    output: Vec<String>,
    status: Status,
    running: Option<Receiver<Msg>>,
    /// A flash that writes, waiting for the user's confirmation.
    confirm: Option<Op>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            form: Form::default(),
            catalog: catalog::builtin().map_err(|err| err.to_string()),
            output: Vec::new(),
            status: Status::Idle,
            running: None,
            confirm: None,
        }
    }
}

/// The catalog picker's text for `artifact`.
fn artifact_label(artifact: &Artifact) -> String {
    let source = if artifact.urls.is_empty() && artifact.mirror_urls.is_empty() {
        " (needs a downloaded copy)"
    } else {
        ""
    };
    format!("{} {}{source}", artifact.model, artifact.version)
}

/// A single-line text field that fills the row.
fn field(ui: &mut egui::Ui, text: &mut String, hint: &str) -> egui::Response {
    ui.add(
        egui::TextEdit::singleline(text)
            .hint_text(hint)
            .desired_width(f32::INFINITY),
    )
}

/// The hint of a path field.
const DROP_HINT: &str = "Type a path, or drop a file on the window";

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
        || "the first connected GameSir controller".to_owned(),
        |pid| format!("the controller with USB product ID {pid:04x}"),
    );
    format!(
        "Install {} on {pad}?\n\nA bad install can stop the controller. Keep the controller \
         connected until the output says \"done; replug the pad\".",
        image.display()
    )
}

impl App {
    const fn busy(&self) -> bool {
        self.running.is_some() || self.confirm.is_some()
    }

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
                Ok(true) => Status::Done,
                Ok(false) => Status::CheckFailed,
                Err(err) => {
                    self.output.push(format!("error: {err}"));
                    Status::Failed
                }
            };
        }
    }

    fn start(&mut self, ctx: &egui::Context) {
        match self.form.op() {
            Ok(op) if writes(&op) => self.confirm = Some(op),
            Ok(op) => self.launch(op, ctx),
            Err(err) => self.status = Status::Invalid(err),
        }
    }

    fn launch(&mut self, op: Op, ctx: &egui::Context) {
        self.output.clear();
        self.status = Status::Running;
        self.running = Some(spawn(op, ctx.clone()));
    }

    /// The write confirmation dialog, while a flash waits for it.
    fn confirmation(&mut self, ctx: &egui::Context) {
        let Some(op) = self.confirm.take() else {
            return;
        };
        let mut answer = None;
        egui::Modal::new(egui::Id::new("confirm-write")).show(ctx, |ui| {
            ui.set_max_width(420.0);
            ui.heading("Install firmware?");
            ui.add_space(4.0);
            ui.label(confirm_text(&op));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let error = ui.visuals().error_fg_color;
                if ui
                    .button(egui::RichText::new("Install").color(error))
                    .clicked()
                {
                    answer = Some(true);
                }
                if ui.button("Cancel").clicked() {
                    answer = Some(false);
                }
            });
        });
        match answer {
            Some(true) => self.launch(op, ctx),
            Some(false) => self.status = Status::Cancelled,
            None => self.confirm = Some(op),
        }
    }

    /// The side panel: one entry per operation, in groups.
    fn operations(&mut self, ui: &mut egui::Ui) {
        let busy = self.busy();
        for group in Group::ALL {
            ui.add_space(6.0);
            ui.strong(group.title());
            for kind in group.kinds() {
                let selected = self.form.kind == *kind;
                let entry = egui::Button::selectable(selected, kind.name());
                if ui.add_enabled(!busy, entry).clicked() && !selected {
                    self.form.kind = *kind;
                    self.status = Status::Idle;
                }
            }
        }
    }

    /// The Fetch firmware picker, or a text field when the catalog cannot be read.
    fn artifact(&mut self, ui: &mut egui::Ui) {
        let list = match &self.catalog {
            Ok(list) => list,
            Err(err) => {
                ui.vertical(|ui| {
                    field(ui, &mut self.form.first, "Firmware name");
                    ui.colored_label(ui.visuals().error_fg_color, err);
                });
                return;
            }
        };
        let selected = list
            .iter()
            .find(|artifact| artifact.id == self.form.first.trim())
            .map_or_else(|| "Choose your controller".to_owned(), artifact_label);
        egui::ComboBox::from_id_salt("artifact")
            .width(ui.available_width())
            .selected_text(selected)
            .show_ui(ui, |ui| {
                for artifact in list {
                    ui.selectable_value(
                        &mut self.form.first,
                        artifact.id.clone(),
                        artifact_label(artifact),
                    )
                    .on_hover_text(&artifact.id);
                }
            });
    }

    fn inputs(&mut self, ui: &mut egui::Ui) {
        let kind = self.form.kind;
        let (first, second) = kind.paths();
        egui::Grid::new("inputs")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .min_col_width(110.0)
            .show(ui, |ui| {
                if let Some(label) = first {
                    ui.label(label);
                    if kind == Kind::Fetch {
                        self.artifact(ui);
                    } else {
                        field(ui, &mut self.form.first, DROP_HINT);
                    }
                    ui.end_row();
                }
                if let Some(label) = second {
                    ui.label(label);
                    field(ui, &mut self.form.second, "Type a path");
                    ui.end_row();
                }
                if kind == Kind::Fetch {
                    ui.label("Downloaded copy");
                    field(
                        ui,
                        &mut self.form.from,
                        "Optional. Leave empty to download from GameSir",
                    );
                    ui.end_row();
                }
                if kind == Kind::Crc {
                    ui.label("Start address");
                    field(ui, &mut self.form.addr, "For example 0x3000");
                    ui.end_row();
                    ui.label("Length");
                    field(ui, &mut self.form.len, "For example 0x2d000");
                    ui.end_row();
                }
            });
        if kind == Kind::Fetch && self.needs_copy() {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                "GameSir gives no download link for this firmware. Choose a downloaded copy.",
            );
        }
        if kind == Kind::Flash {
            ui.add_space(4.0);
            ui.checkbox(&mut self.form.write, "Write firmware")
                .on_hover_text("Clear this box to check the file and the controller only.");
            ui.checkbox(
                &mut self.form.keep_region_a,
                "Install when the start area differs",
            )
            .on_hover_text(
                "The start area of the controller is different from the file. Select this \
                     only when you know that the file is correct for this controller.",
            );
        }
        if kind.pad() {
            self.connection(ui);
        }
    }

    /// Whether the chosen download has no link, so Fetch needs a downloaded copy.
    fn needs_copy(&self) -> bool {
        let id = self.form.first.trim();
        self.form.from.trim().is_empty()
            && self.catalog.as_ref().is_ok_and(|list| {
                list.iter().any(|artifact| {
                    artifact.id == id && artifact.urls.is_empty() && artifact.mirror_urls.is_empty()
                })
            })
    }

    /// The USB connection options.
    fn connection(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new("Advanced connection settings").show(ui, |ui| {
            egui::Grid::new("connection")
                .num_columns(2)
                .spacing([12.0, 8.0])
                .min_col_width(110.0)
                .show(ui, |ui| {
                    ui.label("USB product ID");
                    field(
                        ui,
                        &mut self.form.link.pid,
                        "Hex. Leave empty to use the first controller",
                    );
                    ui.end_row();
                    ui.label("Message flags");
                    field(ui, &mut self.form.link.flags, "Change only when told to");
                    ui.end_row();
                    ui.label("Session key");
                    field(
                        ui,
                        &mut self.form.link.session_key,
                        "Hex. Leave empty to connect again",
                    )
                    .on_hover_text(
                        "The controller answers one connection per power-up. To connect again \
                         without a replug, type the key from the last output.",
                    );
                    ui.end_row();
                });
            ui.checkbox(&mut self.form.link.power_on, "Wake the controller first");
            ui.checkbox(&mut self.form.link.verbose, "Show all USB messages");
        });
    }

    fn controls(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let text = self.form.kind.action(self.form.write);
            let text = if writes_flash(&self.form) {
                egui::RichText::new(text).color(ui.visuals().error_fg_color)
            } else {
                egui::RichText::new(text)
            };
            if ui
                .add_enabled(!self.busy(), egui::Button::new(text))
                .clicked()
            {
                self.start(ui.ctx());
            }
            self.status.show(ui);
        });
    }

    fn output(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.strong("Output");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let some = !self.output.is_empty();
                if ui
                    .add_enabled(some && !self.busy(), egui::Button::new("Clear"))
                    .clicked()
                {
                    self.output.clear();
                    self.status = Status::Idle;
                }
                if ui.add_enabled(some, egui::Button::new("Copy")).clicked() {
                    ui.ctx().copy_text(self.output.join("\n"));
                }
            });
        });
        egui::Frame::group(ui.style()).show(ui, |ui| {
            egui::ScrollArea::both()
                .stick_to_bottom(true)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if self.output.is_empty() {
                        ui.weak(format!(
                            "Fill in the fields and click {}. The result shows here.",
                            self.form.kind.action(self.form.write)
                        ));
                    }
                    for line in &self.output {
                        ui.monospace(line);
                    }
                });
        });
    }

    /// A dropped file goes into the first path field, or into Downloaded copy for Fetch.
    fn dropped_file(&mut self, ctx: &egui::Context) {
        let dropped = ctx.input(|input| input.raw.dropped_files.clone());
        let Some(text) = dropped
            .first()
            .map(|file| file.path().display().to_string())
        else {
            return;
        };
        match self.form.kind {
            Kind::Fetch => self.form.from = text,
            kind if kind.paths().0.is_some() => self.form.first = text,
            _ => {}
        }
    }
}

/// Whether the form describes a flash that writes.
fn writes_flash(form: &Form) -> bool {
    form.kind == Kind::Flash && form.write
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll();
        self.dropped_file(&ctx);
        self.confirmation(&ctx);
        egui::Panel::left("operations")
            .resizable(false)
            .default_size(180.0)
            .show(ui, |ui| self.operations(ui));
        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading(self.form.kind.name());
            ui.label(self.form.kind.about());
            ui.add_space(8.0);
            self.inputs(ui);
            ui.add_space(8.0);
            self.controls(ui);
            ui.add_space(8.0);
            self.output(ui);
        });
    }
}

/// Opens the window.
#[must_use]
pub fn run() -> ExitCode {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("GameSir Firmware Tool")
            .with_inner_size([900.0, 620.0])
            .with_min_inner_size([640.0, 420.0])
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

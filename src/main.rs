#![windows_subsystem = "windows"]

mod app;
mod bru;
mod collection;
mod curl;
mod exec;
mod grpc;
mod http;
mod interp;
mod loadtest;
mod model;
mod script;
mod ui;
mod ws;
mod yml;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default().with_inner_size([1440.0, 880.0]).with_min_inner_size([900.0, 560.0]).with_title("ZenProbe"),
        ..Default::default()
    };
    eframe::run_native(
        "ZenProbe",
        options,
        Box::new(|cc| {
            ui::setup(&cc.egui_ctx);
            Ok(Box::new(app::ZenApp::new()))
        }),
    )
}

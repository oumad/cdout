use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;

#[test]
fn click_updates_label_and_snapshot() {
    let mut count = 0u32;
    let mut harness = Harness::new_ui(|ui| {
        if ui.button("Increment").clicked() {
            count += 1;
        }
        ui.label(format!("count={count}"));
        ui.add(eframe::egui::TextEdit::singleline(&mut String::from("abc")).hint_text("prompt"));
    });
    harness.get_by_label("count=0");
    harness.get_by_label("Increment").click();
    harness.run();
    harness.get_by_label("count=1");
    harness.snapshot("counter");
}

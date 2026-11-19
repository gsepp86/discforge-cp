use eframe::egui::{self, Color32, Pos2, Shape, Stroke};
use std::f32::consts::TAU;

/// Disegna un disco che ruota. I riflessi arcobaleno scorrono sulla superficie.
pub fn mostra(ui: &mut egui::Ui, angolo: f32, _attivo: bool) {
    let dim = 180.0;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(dim, dim), egui::Sense::hover());
    let p = ui.painter_at(rect);
    let c = rect.center();
    let r = dim * 0.48;
    let sfondo = ui.visuals().panel_fill;

    // Corpo argenteo
    p.circle_filled(c, r, Color32::from_rgb(200, 206, 216));

    // Spicchi iridescenti che ruotano con il disco
    const SPICCHI: usize = 36;
    let passo = TAU / SPICCHI as f32;
    for i in 0..SPICCHI {
        let a0 = angolo + i as f32 * passo;
        let a1 = a0 + passo;
        let tinta = (i as f32 / SPICCHI as f32 + angolo / TAU).fract();
        let colore: Color32 = egui::ecolor::Hsva::new(tinta, 0.55, 1.0, 0.35).into();
        p.add(Shape::convex_polygon(
            vec![c, punto(c, r, a0), punto(c, r, a1)],
            colore,
            Stroke::NONE,
        ));
    }

    // Tracce concentriche
    for k in [0.85, 0.72, 0.58, 0.45] {
        p.circle_stroke(c, r * k, Stroke::new(1.0, Color32::from_black_alpha(35)));
    }

    // Foro centrale e bordo
    p.circle_filled(c, r * 0.22, sfondo);
    p.circle_stroke(c, r * 0.22, Stroke::new(1.5, Color32::from_gray(120)));
    p.circle_stroke(c, r, Stroke::new(1.5, Color32::from_gray(140)));
}

fn punto(centro: Pos2, raggio: f32, angolo: f32) -> Pos2 {
    Pos2::new(
        centro.x + raggio * angolo.cos(),
        centro.y + raggio * angolo.sin(),
    )
}

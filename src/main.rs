#![windows_subsystem = "windows"]

mod burn;
mod disc;

use burn::{BurnJob, BurnMsg, Sorgente};
use eframe::egui;
use std::path::PathBuf;

const VERSION: &str = env!("CARGO_PKG_VERSION");
/// Margine stimato per le strutture del file system (tabelle, metadati).
const OVERHEAD: f64 = 1.03;

#[derive(Clone, Copy, PartialEq)]
enum Supporto {
    Cd,
    Dvd,
    DvdDl,
    Bd,
}

impl Supporto {
    fn nome(self) -> &'static str {
        match self {
            Supporto::Cd => "CD (700 MB)",
            Supporto::Dvd => "DVD (4,7 GB)",
            Supporto::DvdDl => "DVD DL (8,5 GB)",
            Supporto::Bd => "Blu-ray (25 GB)",
        }
    }

    fn capacita(self) -> u64 {
        match self {
            Supporto::Cd => 700_000_000,
            Supporto::Dvd => 4_700_000_000,
            Supporto::DvdDl => 8_500_000_000,
            Supporto::Bd => 25_000_000_000,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Modalita {
    Dati,
    Immagine,
    Copia,
}

#[derive(Clone, Copy, PartialEq)]
enum FileSystem {
    Iso9660Joliet,
    Udf,
}

impl FileSystem {
    fn nome(self) -> &'static str {
        match self {
            FileSystem::Iso9660Joliet => "ISO 9660 + Joliet",
            FileSystem::Udf => "UDF",
        }
    }
}

#[derive(PartialEq)]
enum Stato {
    Pronto,
    Masterizzando,
    Completato,
    Errore(String),
}

/// Un file o una cartella aggiunta alla lista di masterizzazione.
struct Voce {
    percorso: PathBuf,
    cartella: bool,
    byte: u64,
    selezionata: bool,
}

impl Voce {
    fn nome(&self) -> String {
        self.percorso
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.percorso.display().to_string())
    }
}

struct App {
    tema_scuro: bool,
    mostra_impostazioni: bool,
    modalita: Modalita,
    supporto: Supporto,
    // Modalità immagine
    iso: Option<PathBuf>,
    dimensione_iso: u64,
    // Modalità dati
    voci: Vec<Voce>,
    etichetta: String,
    filesystem: FileSystem,
    multisessione: bool,
    // Modalità copia disco
    unita: Vec<String>,
    unita_idx: usize,
    verifica_copia: bool,
    // Stato della masterizzazione
    job: Option<BurnJob>,
    progresso: f32,
    fase: String,
    stato: Stato,
    // Animazione
    disco_angolo: f32,
    disco_velocita: f32,
}

impl Default for App {
    fn default() -> Self {
        Self {
            tema_scuro: true,
            mostra_impostazioni: false,
            modalita: Modalita::Dati,
            supporto: Supporto::Dvd,
            iso: None,
            dimensione_iso: 0,
            voci: Vec::new(),
            etichetta: "DISCFORGE".into(),
            filesystem: FileSystem::Iso9660Joliet,
            multisessione: false,
            unita: burn::unita_ottiche(),
            unita_idx: 0,
            verifica_copia: true,
            job: None,
            progresso: 0.0,
            fase: String::new(),
            stato: Stato::Pronto,
            disco_angolo: 0.0,
            disco_velocita: 0.0,
        }
    }
}

impl App {
    fn seleziona_iso(&mut self) {
        if let Some(percorso) = rfd::FileDialog::new()
            .add_filter("Immagini ISO", &["iso"])
            .pick_file()
        {
            self.dimensione_iso = std::fs::metadata(&percorso).map(|m| m.len()).unwrap_or(0);
            self.iso = Some(percorso);
            self.stato = Stato::Pronto;
        }
    }

    fn aggiungi_file(&mut self) {
        if let Some(percorsi) = rfd::FileDialog::new().pick_files() {
            for p in percorsi {
                self.aggiungi_voce(p);
            }
        }
    }

    fn aggiungi_cartella(&mut self) {
        if let Some(p) = rfd::FileDialog::new().pick_folder() {
            self.aggiungi_voce(p);
        }
    }

    fn aggiungi_voce(&mut self, percorso: PathBuf) {
        if self.voci.iter().any(|v| v.percorso == percorso) {
            return;
        }
        let byte = burn::dimensione(&percorso);
        let cartella = percorso.is_dir();
        self.voci.push(Voce { percorso, cartella, byte, selezionata: false });
        self.stato = Stato::Pronto;
    }

    fn rimuovi_selezionate(&mut self) {
        self.voci.retain(|v| !v.selezionata);
    }

    fn totale_dati(&self) -> u64 {
        self.voci.iter().map(|v| v.byte).sum()
    }

    /// Spazio che la masterizzazione occuperà sul disco.
    fn occupato(&self) -> u64 {
        match self.modalita {
            Modalita::Immagine => self.dimensione_iso,
            Modalita::Dati => (self.totale_dati() as f64 * OVERHEAD) as u64,
            Modalita::Copia => 0,
        }
    }

    fn pronto(&self) -> bool {
        match self.modalita {
            Modalita::Immagine => self.iso.is_some(),
            Modalita::Dati => !self.voci.is_empty() && !self.etichetta.trim().is_empty(),
            Modalita::Copia => !self.unita.is_empty(),
        }
    }

    fn avvia(&mut self) {
        let sorgente = match self.modalita {
            Modalita::Immagine => match &self.iso {
                Some(p) => Sorgente::Immagine(p.clone()),
                None => return,
            },
            Modalita::Dati => Sorgente::Dati {
                voci: self.voci.iter().map(|v| v.percorso.clone()).collect(),
                etichetta: self.etichetta.trim().to_string(),
                multisessione: self.multisessione,
                udf: self.filesystem == FileSystem::Udf,
            },
            Modalita::Copia => match self.unita.get(self.unita_idx) {
                Some(u) => Sorgente::Copia {
                    unita: u.clone(),
                    verifica: self.verifica_copia,
                },
                None => return,
            },
        };
        self.job = Some(BurnJob::start(sorgente));
        self.progresso = 0.0;
        self.fase = String::new();
        self.stato = Stato::Masterizzando;
    }

    fn annulla(&mut self) {
        if let Some(job) = &self.job {
            job.annulla();
        }
    }

    fn aggiorna_job(&mut self) {
        let mut termina = false;
        if let Some(job) = &self.job {
            while let Ok(msg) = job.rx.try_recv() {
                match msg {
                    BurnMsg::Progresso(p) => self.progresso = p,
                    BurnMsg::Fase(f) => self.fase = f,
                    BurnMsg::Fine(Ok(())) => {
                        self.progresso = 1.0;
                        self.stato = Stato::Completato;
                        termina = true;
                    }
                    BurnMsg::Fine(Err(e)) => {
                        self.stato = Stato::Errore(e);
                        termina = true;
                    }
                }
            }
        }
        if termina {
            self.job = None;
        }
    }
}

fn formato_byte(b: u64) -> String {
    if b >= 1_000_000_000 {
        format!("{:.2} GB", b as f64 / 1e9)
    } else if b >= 1_000_000 {
        format!("{:.1} MB", b as f64 / 1e6)
    } else {
        format!("{} KB", b / 1000)
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.set_visuals(if self.tema_scuro {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        });

        self.aggiorna_job();

        // Animazione: il disco accelera durante la masterizzazione e frena a fine lavoro
        let dt = ctx.input(|i| i.stable_dt).min(0.1);
        let in_corso = self.job.is_some();
        let velocita_target = if in_corso { 10.0 } else { 0.0 };
        self.disco_velocita += (velocita_target - self.disco_velocita) * (dt * 3.0).min(1.0);
        self.disco_angolo =
            (self.disco_angolo + self.disco_velocita * dt) % std::f32::consts::TAU;
        if in_corso || self.disco_velocita > 0.01 {
            ctx.request_repaint();
        }

        let occupato = self.occupato();
        let capacita = self.supporto.capacita();
        let troppo_grande = occupato > capacita;

        egui::TopBottomPanel::top("barra").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Discforge CP");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .button("\u{2699}")
                        .on_hover_text("Impostazioni")
                        .clicked()
                    {
                        self.mostra_impostazioni = true;
                    }
                });
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                disc::mostra(ui, self.disco_angolo, in_corso);
            });
            ui.add_space(6.0);

            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.modalita, Modalita::Dati, "Dati");
                ui.selectable_value(&mut self.modalita, Modalita::Immagine, "Immagine ISO");
                ui.selectable_value(&mut self.modalita, Modalita::Copia, "Copia disco");
            });
            ui.separator();

            match self.modalita {
                Modalita::Immagine => {
                    ui.horizontal(|ui| {
                        ui.label("File ISO:");
                        if ui.button("Sfoglia...").clicked() {
                            self.seleziona_iso();
                        }
                    });
                    if let Some(p) = &self.iso {
                        ui.monospace(p.display().to_string());
                        ui.label(format!("Dimensione: {}", formato_byte(self.dimensione_iso)));
                    }
                }
                Modalita::Dati => {
                    ui.horizontal(|ui| {
                        if ui.button("Aggiungi file...").clicked() {
                            self.aggiungi_file();
                        }
                        if ui.button("Aggiungi cartella...").clicked() {
                            self.aggiungi_cartella();
                        }
                        let c_selezionate = self.voci.iter().any(|v| v.selezionata);
                        if ui
                            .add_enabled(c_selezionate, egui::Button::new("Rimuovi"))
                            .clicked()
                        {
                            self.rimuovi_selezionate();
                        }
                        if ui
                            .add_enabled(!self.voci.is_empty(), egui::Button::new("Svuota"))
                            .clicked()
                        {
                            self.voci.clear();
                        }
                    });

                    egui::ScrollArea::vertical()
                        .max_height(150.0)
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            if self.voci.is_empty() {
                                ui.weak("Nessun elemento. Aggiungi file o cartelle.");
                            }
                            for v in self.voci.iter_mut() {
                                let icona = if v.cartella { "[DIR]" } else { "[FILE]" };
                                let testo = format!(
                                    "{icona} {}  ({})",
                                    v.nome(),
                                    formato_byte(v.byte)
                                );
                                ui.checkbox(&mut v.selezionata, testo);
                            }
                        });

                    ui.label(format!(
                        "Totale: {} in {} elementi",
                        formato_byte(self.totale_dati()),
                        self.voci.len()
                    ));

                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.label("Etichetta disco:");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.etichetta)
                                .char_limit(32)
                                .desired_width(200.0),
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.label("File system:");
                        egui::ComboBox::from_id_salt("filesystem")
                            .selected_text(self.filesystem.nome())
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.filesystem,
                                    FileSystem::Iso9660Joliet,
                                    FileSystem::Iso9660Joliet.nome(),
                                );
                                ui.selectable_value(
                                    &mut self.filesystem,
                                    FileSystem::Udf,
                                    FileSystem::Udf.nome(),
                                );
                            });
                    });
                    ui.checkbox(&mut self.multisessione, "Multisessione")
                        .on_hover_text(
                            "Aggiunge una nuova sessione a un disco già scritto, \
                             senza cancellare le sessioni precedenti.",
                        );
                }
                Modalita::Copia => {
                    if self.unita.is_empty() {
                        ui.colored_label(
                            egui::Color32::from_rgb(230, 90, 90),
                            "Nessuna unità ottica rilevata",
                        );
                    } else {
                        ui.horizontal(|ui| {
                            ui.label("Unità sorgente:");
                            let attuale = self.unita[self.unita_idx].clone();
                            egui::ComboBox::from_id_salt("unita")
                                .selected_text(attuale)
                                .show_ui(ui, |ui| {
                                    for (i, nome) in self.unita.iter().enumerate() {
                                        ui.selectable_value(&mut self.unita_idx, i, nome.as_str());
                                    }
                                });
                        });
                    }
                    ui.checkbox(&mut self.verifica_copia, "Verifica dopo la copia");
                    ui.weak(
                        "Inserisci il disco sorgente, poi il disco vuoto di destinazione \
                         nel supporto scelto sotto.",
                    );
                }
            }

            if self.modalita != Modalita::Copia {
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label("Spazio:");
                    let frazione = (occupato as f32 / capacita as f32).min(1.0);
                    let colore = if troppo_grande {
                        egui::Color32::from_rgb(230, 90, 90)
                    } else {
                        egui::Color32::from_rgb(90, 170, 230)
                    };
                    ui.add(
                        egui::ProgressBar::new(frazione)
                            .fill(colore)
                            .text(format!(
                                "{} / {} ({})",
                                formato_byte(occupato),
                                formato_byte(capacita),
                                self.supporto.nome()
                            )),
                    );
                });
            }
            ui.horizontal(|ui| {
                ui.label("Supporto:");
                egui::ComboBox::from_id_salt("supporto")
                    .selected_text(self.supporto.nome())
                    .show_ui(ui, |ui| {
                        for s in [Supporto::Cd, Supporto::Dvd, Supporto::DvdDl, Supporto::Bd] {
                            ui.selectable_value(&mut self.supporto, s, s.nome());
                        }
                    });
            });

            if troppo_grande {
                ui.colored_label(
                    egui::Color32::from_rgb(230, 90, 90),
                    "I dati non entrano nel supporto selezionato",
                );
            }

            ui.add_space(10.0);
            ui.horizontal(|ui| {
                let puo_avviare = self.pronto() && !in_corso && !troppo_grande;
                let etichetta_avvio = match self.modalita {
                    Modalita::Copia => "Copia",
                    _ => "Masterizza",
                };
                if ui
                    .add_enabled(puo_avviare, egui::Button::new(etichetta_avvio))
                    .clicked()
                {
                    self.avvia();
                }
                if ui
                    .add_enabled(in_corso, egui::Button::new("Annulla"))
                    .clicked()
                {
                    self.annulla();
                }
            });

            if in_corso || self.stato != Stato::Pronto {
                ui.add_space(6.0);
                ui.add(egui::ProgressBar::new(self.progresso).show_percentage());
            }

            let testo = match &self.stato {
                Stato::Pronto => "Pronto".to_string(),
                Stato::Masterizzando => {
                    if self.fase.is_empty() {
                        "Avvio...".to_string()
                    } else {
                        self.fase.clone()
                    }
                }
                Stato::Completato => "Operazione completata".to_string(),
                Stato::Errore(e) => format!("Errore: {e}"),
            };
            ui.add_space(4.0);
            ui.label(testo);
        });

        egui::Window::new("Impostazioni")
            .open(&mut self.mostra_impostazioni)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.heading("Tema");
                ui.horizontal(|ui| {
                    ui.radio_value(&mut self.tema_scuro, true, "Scuro");
                    ui.radio_value(&mut self.tema_scuro, false, "Chiaro");
                });

                ui.separator();

                ui.heading("Info");
                ui.label("Discforge CP");
                ui.label(format!("Versione {VERSION}"));
                ui.add_space(6.0);
                ui.label("Con Discforge CP puoi:");
                ui.label("- masterizzare immagini ISO esistenti su CD, DVD o Blu-ray;");
                ui.label("- creare dischi dati da singoli file o da intere cartelle;");
                ui.label("- scegliere il file system (ISO 9660 con Joliet oppure UDF);");
                ui.label("- aggiungere una sessione a un disco già scritto (multisessione);");
                ui.label("- copiare un disco ottico su un disco vuoto, con verifica opzionale;");
                ui.label("- controllare lo spazio disponibile prima di avviare la scrittura;");
                ui.label("- seguire il progresso e annullare l'operazione in qualsiasi momento.");
                ui.add_space(6.0);
                ui.weak("Scritto in Rust con interfaccia egui.");
            });
    }
}

fn main() -> eframe::Result {
    let opzioni = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([540.0, 740.0])
            .with_min_inner_size([460.0, 640.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Discforge CP",
        opzioni,
        Box::new(|_cc| Ok(Box::new(App::default()))),
    )
}

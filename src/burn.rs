use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::Duration;

const BLOCCO: usize = 1024 * 1024;
/// Dimensione simulata della copia disco (il disco reale verrà letto settore per settore).
const SIM_COPIA: u64 = 300_000_000;

pub enum BurnMsg {
    Progresso(f32),
    Fase(String),
    Fine(Result<(), String>),
}

/// Cosa fare: masterizzare un'immagine, masterizzare dati, o copiare un disco.
#[derive(Clone)]
pub enum Sorgente {
    Immagine(PathBuf),
    Dati {
        voci: Vec<PathBuf>,
        etichetta: String,
        multisessione: bool,
        udf: bool,
    },
    Copia {
        unita: String,
        verifica: bool,
    },
}

pub struct BurnJob {
    pub rx: mpsc::Receiver<BurnMsg>,
    annulla: Arc<AtomicBool>,
}

impl BurnJob {
    pub fn start(sorgente: Sorgente) -> Self {
        let (tx, rx) = mpsc::channel();
        let annulla = Arc::new(AtomicBool::new(false));
        let flag = annulla.clone();

        thread::spawn(move || {
            let esito = esegui(&sorgente, &flag, &tx);
            let _ = tx.send(BurnMsg::Fine(esito));
        });

        Self { rx, annulla }
    }

    pub fn annulla(&self) {
        self.annulla.store(true, Ordering::Relaxed);
    }
}

/// Unità ottiche disponibili per la copia disco.
pub fn unita_ottiche() -> Vec<String> {
    // TODO: IMAPI2 MsftDiscMaster2 -> MsftDiscRecorder2 per elencare le unità reali.
    vec!["D: (unità simulata)".to_string(), "E: (unità simulata)".to_string()]
}

/// Dimensione totale di un file o di una cartella (ricorsiva).
pub fn dimensione(percorso: &Path) -> u64 {
    match fs::metadata(percorso) {
        Ok(m) if m.is_dir() => fs::read_dir(percorso)
            .map(|rd| rd.flatten().map(|e| dimensione(&e.path())).sum())
            .unwrap_or(0),
        Ok(m) => m.len(),
        Err(_) => 0,
    }
}

/// Elenca tutti i file contenuti in un percorso, in modo ricorsivo.
fn raccogli(percorso: &Path, out: &mut Vec<PathBuf>) {
    if percorso.is_dir() {
        if let Ok(rd) = fs::read_dir(percorso) {
            for e in rd.flatten() {
                raccogli(&e.path(), out);
            }
        }
    } else {
        out.push(percorso.to_path_buf());
    }
}

fn esegui(sorgente: &Sorgente, annulla: &AtomicBool, tx: &mpsc::Sender<BurnMsg>) -> Result<(), String> {
    match sorgente {
        Sorgente::Immagine(percorso) => scrivi_immagine(percorso, annulla, tx),
        Sorgente::Dati { voci, etichetta, multisessione, udf } => {
            let fs_nome = if *udf { "UDF" } else { "ISO 9660 + Joliet" };

            // Fase 1: costruzione del file system con l'elenco dei file.
            let _ = tx.send(BurnMsg::Fase(format!("Creazione file system ({fs_nome})...")));
            let mut file = Vec::new();
            for v in voci {
                raccogli(v, &mut file);
            }
            if file.is_empty() {
                return Err("Nessun file da masterizzare".into());
            }
            let n = file.len();
            for (i, f) in file.iter().enumerate() {
                if annulla.load(Ordering::Relaxed) {
                    return Err("Operazione annullata".into());
                }
                // TODO: aggiungere il file al file system con IMAPI2 (IFsiDirectoryItem / AddTree).
                thread::sleep(Duration::from_millis(2));
                let _ = tx.send(BurnMsg::Progresso(0.3 * (i + 1) as f32 / n as f32));
            }

            // Fase 2: multisessione, se richiesta.
            if *multisessione {
                let _ = tx.send(BurnMsg::Fase("Lettura delle sessioni del disco...".into()));
                // TODO: IMAPI2 IMultisession::IsSupportedMultisession e ImportedFileSystem.
                thread::sleep(Duration::from_millis(300));
            }

            // Fase 3: scrittura. Il totale è la somma dei byte dei file.
            let totale: u64 = file
                .iter()
                .map(|f| fs::metadata(f).map(|m| m.len()).unwrap_or(0))
                .sum();
            let _ = tx.send(BurnMsg::Fase(format!("Masterizzazione \"{etichetta}\"...")));
            // TODO: IMAPI2 IDiscFormat2Data::Write con lo stream del file system.
            scrivi_blocchi(totale.max(1), 0.3, 1.0, annulla, tx)?;

            let _ = tx.send(BurnMsg::Fase("Verifica...".into()));
            // TODO: rilettura del disco e confronto degli hash.
            thread::sleep(Duration::from_millis(300));
            Ok(())
        }
        Sorgente::Copia { unita, verifica } => {
            // Fase 1: lettura del disco sorgente in un'immagine temporanea.
            let _ = tx.send(BurnMsg::Fase(format!("Lettura da {unita}...")));
            // TODO: leggere i settori dell'unità con IMAPI2 / lettura raw del dispositivo.
            scrivi_blocchi(SIM_COPIA, 0.0, 0.4, annulla, tx)?;

            // Fase 2: scrittura sul disco vuoto.
            let _ = tx.send(BurnMsg::Fase("Scrittura del disco di destinazione...".into()));
            // TODO: IMAPI2 IDiscFormat2RawCD / IDiscFormat2Data con lo stream dell'immagine.
            let fine_scrittura = if *verifica { 0.85 } else { 1.0 };
            scrivi_blocchi(SIM_COPIA, 0.4, fine_scrittura, annulla, tx)?;

            // Fase 3 (opzionale): verifica del disco scritto.
            if *verifica {
                let _ = tx.send(BurnMsg::Fase("Verifica del disco copiato...".into()));
                // TODO: confronto hash tra sorgente e destinazione.
                scrivi_blocchi(SIM_COPIA, 0.85, 1.0, annulla, tx)?;
            }
            Ok(())
        }
    }
}

fn scrivi_immagine(percorso: &Path, annulla: &AtomicBool, tx: &mpsc::Sender<BurnMsg>) -> Result<(), String> {
    let mut file = File::open(percorso).map_err(|e| format!("Impossibile aprire l'ISO: {e}"))?;
    let totale = file.metadata().map_err(|e| e.to_string())?.len().max(1);
    let mut buf = vec![0u8; BLOCCO];
    let mut letti: u64 = 0;

    loop {
        if annulla.load(Ordering::Relaxed) {
            return Err("Operazione annullata".into());
        }

        let n = file
            .read(&mut buf)
            .map_err(|e| format!("Errore di lettura: {e}"))?;
        if n == 0 {
            break;
        }

        // TODO: scrivere buf[..n] sul disco tramite IMAPI2 (modalità raw).
        thread::sleep(Duration::from_millis(8));

        letti += n as u64;
        let _ = tx.send(BurnMsg::Progresso(letti as f32 / totale as f32));
    }

    Ok(())
}

/// Simula la scrittura a blocchi di `totale` byte, con progresso nell'intervallo [da, a].
fn scrivi_blocchi(
    totale: u64,
    da: f32,
    a: f32,
    annulla: &AtomicBool,
    tx: &mpsc::Sender<BurnMsg>,
) -> Result<(), String> {
    let mut scritti: u64 = 0;
    while scritti < totale {
        if annulla.load(Ordering::Relaxed) {
            return Err("Operazione annullata".into());
        }
        let n = (BLOCCO as u64).min(totale - scritti);
        thread::sleep(Duration::from_millis(8));
        scritti += n;
        let frazione = scritti as f32 / totale as f32;
        let _ = tx.send(BurnMsg::Progresso(da + (a - da) * frazione));
    }
    Ok(())
}

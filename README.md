# Discforge CP 1.0

Discforge CP è un masterizzatore per Windows scritto in Rust.

## Funzioni
- Modalità **Dati**: aggiunta di file e cartelle, calcolo della dimensione, etichetta del disco,
  scelta del file system (ISO 9660 + Joliet oppure UDF), opzione multisessione.
- Modalità **Immagine ISO**: masterizzazione di file .iso esistenti.
- Modalità **Copia disco**: lettura di un disco sorgente e scrittura su un disco vuoto, con verifica opzionale.
- Controllo della capacità in base al supporto (CD, DVD, DVD DL, Blu-ray).
- Masterizzazione in thread separato, con fasi, progresso e annullamento.
- Impostazioni (ingranaggio in alto a destra): tema chiaro/scuro e sezione Info.

## Stato
La scrittura reale è ancora simulata. I punti da completare sono marcati con `TODO`
in `src/burn.rs` e riguardano IMAPI2 (IFileSystemImage, IMultisession, IDiscFormat2Data).

## Compilazione
    cargo run --release

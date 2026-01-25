# WinBoat Bridge

## Cos'è WinBoat Bridge?

**WinBoat Bridge** è uno strumento progettato per permettere a un sistema **Linux** di eseguire comandi direttamente all'interno di un ambiente **Windows** virtualizzato (WinBoat), senza dover configurare complessi servizi SSH o WinRM manualmente ogni volta.

Immaginalo come un "telecomando": tu digiti il comando sul tuo terminale Linux, e questo viene eseguito istantaneamente sulla macchina Windows, restituendoti il risultato come se fossi seduto davanti a quel PC.

È pensato specificamente per **scenari di Continuous Integration (CI)**, dove è necessario automatizzare test o script su Windows pilotandoli da un ambiente Linux.

## Compatibilità

Questo software è composto da un unico eseguibile che può funzionare in due modalità:

1.  **Client (Linux)**: Viene eseguito sulla tua macchina Linux. Invia i comandi e riceve le risposte.
2.  **Server (Windows)**: Viene eseguito sulla macchina Windows. Riceve i comandi, li esegue e rispedisce l'output.

> **Nota Tecnica**: Il sistema è "intelligente". Se il server su Windows non è attivo, il client Linux è in grado di avviarlo automaticamente (tramite *evil-winrm*) senza che tu debba fare nulla.

## La Cartella Condivisa

Perché tutto funzioni "magicamente", Linux e Windows devono condividere una cartella specifica dove risiede l'eseguibile del server.

*   **Percorso Linux**: `/home/gianca/rust/winboat-bridge` (o dove hai clonato il progetto)
*   **Percorso Windows**: `C:\Users\gianca\Desktop\Shared\rust\winboat-bridge`

Quando compili il progetto su Linux, l'eseguibile per Windows viene creato direttamente in questa cartella condivisa, così la macchina Windows può vederlo ed eseguirlo immediatamente.

## Come si Usa

### 1. Compilazione (Cross-Compiling)

Prima di tutto, devi creare l'eseguibile per Windows lavorando da Linux.
Poiché siamo su **NixOS**, la compilazione incrociata richiede librerie specifiche. Abbiamo creato uno script apposito per facilitare il compito.

Esegui questo comando nella cartella del progetto:

```bash
./build_windows.sh
```

*Questo script imposta i flag corretti per il linker e crea `winboat-bridge.exe` nella cartella condivisa.*

### 2. Dove sono gli eseguibili?

Una volta compilato, i file si trovano in percorsi precisi. È importante usare questi per gli script di produzione/CI invece di ricompilare ogni volta.

*   **Eseguibile Windows (Server)**:
    *   Percorso Linux: `target/x86_64-pc-windows-gnu/release/winboat-bridge.exe`
    *   Percorso Windows: `C:\Users\gianca\Desktop\Shared\rust\winboat-bridge\target\x86_64-pc-windows-gnu\release\winboat-bridge.exe`

*   **Eseguibile Linux (Client)**:
    *   Per ottenerlo esegui: `cargo build --release`
    *   Percorso: `target/release/winboat-bridge`

### 3. Installazione Globale (Eseguire da ovunque)

Per poter digitare semplicemente `winboat-bridge` da qualsiasi cartella senza dover specificare tutto il percorso, devi installarlo nel tuo sistema.

**Metodo consigliato per sviluppo**: usa un link simbolico invece di copiare il file. In questo modo ogni ricompilazione sarà immediatamente disponibile:

```bash
# Rimuovi eventuale copia precedente
rm -f ~/.local/bin/winboat-bridge

# Crea il link simbolico
ln -s $(pwd)/target/release/winboat-bridge ~/.local/bin/winboat-bridge
```

*Nota: Assicurati che `~/.local/bin` sia nel tuo PATH (di solito lo è su Linux moderno).*

**Vantaggi del link simbolico:**
- Ogni `cargo build --release aggiorna immediatamente l'eseguibile globale
- Non devi ricopiare il file dopo ogni modifica
- Ideale per ciclo di sviluppo rapido

### 3.1. Port Mapping Docker Compose

Il progetto WinBoat usa Docker Compose con mappature delle porte specifiche. Assicurati che il tuo `docker-compose.yml` contenga le porte necessarie per winboat-bridge:

```yaml
services:
  windows:
    # ... altre configurazioni
    ports:
      - 127.0.0.1:47320:5985    # WinRM (per bootstrap)
      - 127.0.0.1:47330:5330    # winboat-bridge server (container port 5330 → host port 47330)
      # ... altre porte
```

**Spiegazione delle porte importanti:**
- **5330** (container): Porta su cui ascolta il server winboat-bridge dentro Windows
- **47330** (host): Porta su cui il client Linux si connette (mappata a 5330 nel container)
- **5985** (container): WinRM per il bootstrap automatico del server
- **47320** (host): Porta WinRM su host per il bootstrap

Queste porte devono essere configurate correttamente nel tuo `.env`:
```bash
WINBOAT_CLIENT_PORT=47330  # Porta host a cui si connette il client
WINBOAT_SERVER_PORT=5330   # Porta su cui ascolta il server nel container
```

Una volta fatto questo, puoi testare se funziona digitando:

```bash
winboat-bridge
```

Se tutto è corretto, vedrai un messaggio di aiuto con gli esempi di utilizzo.

### 4. "Cargo Run" vs Eseguibile Diretto

Negli esempi precedenti abbiamo usato `cargo run` per comodità, ma in un ambiente professionale (CI/CD) **non dovresti usarlo**.

*   **Cargo Run**: Compila il programma ogni volta prima di eseguirlo. È lento e serve agli sviluppatori.
*   **Eseguibile Diretto**: È istantaneo. È quello che devi usare nei tuoi script di automazione.

**Come usare l'eseguibile diretto (Consigliato per Tecnici):**

Invece di `cargo run -- -c "..."`, usa direttamente il percorso del file compilato:

```bash
# Esempio Professionale
./target/release/winboat-bridge -c "dir"
```

### 4. Esecuzione Comandi (Esempi Pratici)

Ecco come usare il bridge. Useremo la sintassi con l'eseguibile diretto per simulare uno scenario reale.

#### Esempio 1: Vedere i file in una cartella
Vuoi sapere cosa c'è nella cartella Documenti di Windows?

```bash
./target/release/winboat-bridge -c "dir C:\Users\gianca\Documents"
```

#### Esempio 2: Verificare l'indirizzo IP
Utile per capire se la rete Windows è configurata correttamente.

```bash
./target/release/winboat-bridge -c "ipconfig"
```

#### Esempio 3: Lanciare uno script PowerShell
Se hai uno script complesso, puoi lanciarlo direttamente.

```bash
./target/release/winboat-bridge -c "powershell -File C:\Scripts\test_build.ps1"
```

#### Esempio 4: Chiudere la connessione
Quando hai finito, è buona norma dire al server di chiudersi per liberare risorse.

```bash
./target/release/winboat-bridge -c "quit"
```

## Risoluzione Problemi Comuni

### Il comando "si blocca" o non risponde
Se lanci un comando e il terminale rimane bloccato senza darti risposta:
1.  Premi `Ctrl + C` per interrompere il client Linux.
2.  Riprova a lanciare il comando. Il sistema rileverà che la connessione è "morta" e riavvierà automaticamente il server Windows.

### Errore "Connection refused"
Significa che il server Windows è spento o Docker non ha aperto la porta.
*   **Soluzione**: Rilancia semplicemente il comando. Il client proverà a fare il "bootstrap" (avvio forzato) del server.

### Modificare la configurazione
Se cambi i percorsi delle cartelle, devi aggiornare il file `src/main.rs` dove è definito `exe_path` e ricompilare.

# WinBoat Bridge

WinBoat Bridge è un tool di orchestrazione che permette a un sistema Linux di eseguire comandi all'interno di un ambiente Windows virtualizzato (WinBoat) in modo trasparente.
A differenza di soluzioni standard come SSH o WinRM (usati solo per il bootstrap), WinBoat Bridge offre un canale diretto e veloce, ideale per pipeline di Continuous Integration (CI) e automazione di test.

## 1. Configurazione (File .env)

Il progetto utilizza un file .env per gestire i percorsi e le credenziali.
Copia il file di esempio e personalizzalo prima di iniziare:

```bash
cp .env.example .env
```

**⚠️ IMPORTANTE - Sintassi del file .env:**
- Usare **doppi backslash** (`\\`) per i percorsi Windows
- **NON usare virgolette** per i valori

Esempio corretto:
```bash
WINBOAT_EXE_PATH=C:\\Users\\gianca\\Desktop\\Shared\\progetti\\rust\\winboat-bridge\\target\\release\\winboat-bridge.exe
WINBOAT_LOG_PATH=C:\\Users\\gianca\\server.log
```

Parametri principali:
- **WINBOAT_EXE_PATH**: Percorso assoluto (lato Windows) dove risiede il server
- **WINBOAT_HOST / PORT**: Indirizzo e porta per il bootstrap (WinRM)
- **WINBOAT_CLIENT_PORT**: Porta sul sistema Linux (Host) mappata verso il container
- **WINBOAT_SERVER_PORT**: Porta interna al container Windows su cui ascolta il server

Il file `.env` viene cercato automaticamente in:
1. Directory corrente di lavoro
2. Directory dell'eseguibile
3. Root del progetto (se eseguibile in `target/release`)

## 2. Compilazione

Il progetto genera un unico binario. Deve essere compilato per Windows (Server) e per Linux (Client).

### A. Build per Windows (Server)

Hai due strade, a seconda di dove ti trovi:

#### Opzione 1: Cross-compilazione da Linux (Consigliato per CI/CD)

Se stai lavorando su NixOS o Linux, usa lo script dedicato:

```bash
./build_windows.sh
```

#### Opzione 2: Compilazione nativa su Windows

Se hai accesso diretto al sistema Windows con Rust installato:
1. Apri una PowerShell nella root del progetto.
2. Esegui: `cargo build --release`
3. Troverai il file in `target\release\winboat-bridge.exe`.

Assicurati che il file sia nella cartella condivisa e che il percorso in .env (WINBOAT_EXE_PATH) punti correttamente a questo binario.

### B. Build per Linux (Client)

Sulla tua macchina Linux, compila normalmente:

```bash
cargo build --release
```

## 3. Installazione Globale (Linux)

Per eseguire winboat-bridge da qualsiasi cartella, crea un link simbolico nella directory dei binari utente. Seguendo lo standard XDG, la directory corretta è ~/.local/bin.

```bash
# Crea la directory se non esiste
mkdir -p ~/.local/bin

# Crea un link simbolico verso il binario appena compilato
ln -sf "$(pwd)/target/release/winboat-bridge" ~/.local/bin/winboat-bridge
```

Nota: Assicurati che ~/.local/bin sia nel tuo $PATH (controlla ~/.bashrc o ~/.zshrc).

## 4. Integrazione Docker Compose

Configura il port mapping nel tuo docker-compose.yml per esporre i servizi necessari:

```yaml
services:
  windows:
    ports:
      - "127.0.0.1:47320:5985"  # WinRM (Per il bootstrap automatico)
      - "127.0.0.1:47330:5330"  # WinBoat Bridge (Comunicazione Client-Server)
```

## 5. Esempi di Utilizzo

Una volta configurato il file .env, il client Linux gestirà tutto automaticamente (incluso l'avvio del server su Windows se spento).

Verifica connessione:

```bash
winboat-bridge -c "ipconfig"
```

Esecuzione script PowerShell:

```bash
winboat-bridge -c "powershell -File C:\Scripts\Setup-Test.ps1"
```

## Risoluzione dei Problemi

| Problema              | Causa Possibile          | Soluzione |
|-----------------------|--------------------------|-----------|
| Il comando "appende" | Connessione zombie       | Ctrl+C e riavvia; il client forzerà un nuovo bootstrap. |
| Connection Refused    | Porta mappata errata     | Verifica con `docker ps` che la porta 47330 sia aperta. |
| "WINBOAT_EXE_PATH must be set" | File .env non trovato o sintassi errata | Verifica che il file `.env` esista e usi doppi backslash (`\\`) senza virgolette. Esegui con `--help` per vedere il messaggio `[DEBUG] Loaded .env from: ...` |
| Errore parsing .env   | Sintassi errata          | Usa doppi backslash (`\\`) per i percorsi Windows e NON usare virgolette. |

### Debug del caricamento .env

Per verificare che il file `.env` venga caricato correttamente, esegui:

```bash
./target/release/winboat-bridge --help 2>&1 | grep DEBUG
```

Dovresti vedere:
```
[DEBUG] Loaded .env from: /path/to/.env
```

Se vedi `[WARNING] No .env file found`, controlla che:
1. Il file `.env` esista nella directory corrente, nella directory dell'eseguibile, o nella root del progetto
2. La sintassi sia corretta (doppi backslash, no virgolette)
3. Il file abbia i permessi di lettura corretti
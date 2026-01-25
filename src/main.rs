use clap::{Parser, Subcommand};
use anyhow::{Context, Result};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::process::Command;
use std::process::Stdio;
use std::sync::Arc;
use tokio::sync::Notify;

#[cfg(target_os = "windows")]
mod win_job {
    use winapi::um::jobapi2::{CreateJobObjectW, AssignProcessToJobObject, SetInformationJobObject};
    use winapi::um::winnt::{JobObjectExtendedLimitInformation, JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, HANDLE};
    use std::ptr;
    use std::mem;
    use anyhow::Result;

    // Returns the Job Handle. The Job Object is closed when the handle is dropped (if not leaked),
    // but we want it to persist until we drop it or the process ends.
    // Actually, if we drop the handle, and LIMIT_KILL_ON_JOB_CLOSE is set, the process dies?
    // Yes, "If the job has the JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE flag, closing the last handle to the job object terminates all processes associated with the job."
    // So we need to keep this handle alive as long as the child is alive.
    pub struct JobHandle(HANDLE);
    
    // Send/Sync for Arc? HANDLE is raw pointer basically.
    unsafe impl Send for JobHandle {}
    unsafe impl Sync for JobHandle {}

    impl Drop for JobHandle {
        fn drop(&mut self) {
            unsafe { winapi::um::handleapi::CloseHandle(self.0); }
        }
    }

    pub fn assign_to_new_job(process_handle: std::os::windows::io::RawHandle) -> Result<JobHandle> {
        unsafe {
            let job = CreateJobObjectW(ptr::null_mut(), ptr::null());
            if job.is_null() {
                 return Err(anyhow::anyhow!("Failed to create job object"));
            }
            
            let handle_wrapper = JobHandle(job);

            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;

            let ret = SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &mut info as *mut _ as *mut _,
                mem::size_of_val(&info) as u32,
            );
            
            if ret == 0 {
                 return Err(anyhow::anyhow!("Failed to set job info"));
            }

            let ret = AssignProcessToJobObject(job, process_handle as HANDLE);
             if ret == 0 {
                 return Err(anyhow::anyhow!("Failed to assign process to job"));
            }
            
            Ok(handle_wrapper)
        }
    }
}

#[derive(Parser)]
#[command(name = "winboat-bridge")]
#[command(about = "Bridge to execute commands on WinBoat container via TCP", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Run as server
    #[arg(long)]
    server: bool,

    /// Command to execute (Client mode)
    #[arg(short, long)]
    cmd: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the server (explicit subcommand)
    Server {
        #[arg(short, long, default_value = "5330")]
        port: u16,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    if cli.server || matches!(cli.command, Some(Commands::Server { .. })) {
        let port = if let Some(Commands::Server { port }) = cli.command {
            port
        } else {
            5330
        };
        server_mode(port).await?;
    } else if let Some(cmd) = cli.cmd {
        client_mode(&cmd).await?;
    } else {
        println!("WinBoat Bridge - Remote Command Executor for Windows Containers");
        println!("---------------------------------------------------------------");
        println!("Usage:");
        println!("  winboat-bridge --server          # Run in Server Mode (Windows side)");
        println!("  winboat-bridge -c <COMMAND>      # Execute command remotely (Linux side)");
        println!("");
        println!("Examples:");
        println!("  1. Check remote IP:");
        println!("     winboat-bridge -c \"ipconfig\"");
        println!("");
        println!("  2. List remote directory:");
        println!("     winboat-bridge -c \"dir C:\\Users\"");
        println!("");
        println!("  3. Run PowerShell script:");
        println!("     winboat-bridge -c \"powershell -File C:\\Scripts\\test.ps1\"");
        println!("");
        println!("  4. Close remote server:");
        println!("     winboat-bridge -c \"quit\"");
    }

    Ok(())
}

async fn server_mode(port: u16) -> Result<()> {
    // Force UTF-8 code page on Windows
    #[cfg(target_os = "windows")]
    {
        let _ = Command::new("cmd").args(&["/C", "chcp 65001"]).output().await;
    }

    let addr = format!("0.0.0.0:{}", port);
    let listener = TcpListener::bind(&addr).await?;
    println!("Server listening on {}", addr);

    // Persistent Server Mode
    let shutdown_signal = Arc::new(Notify::new());

    loop {
        let shutdown_signal = shutdown_signal.clone();
        tokio::select! {
            _ = shutdown_signal.notified() => {
                println!("Shutdown signal received. stopping server.");
                break;
            }
            accept_result = listener.accept() => {
                match accept_result {
                    Ok((mut socket, _)) => {
                        tokio::spawn(async move {
                            // Handshake: Send READY
                            if let Err(e) = socket.write_all(b"READY\n").await {
                                eprintln!("Failed to send handshake: {}", e);
                                return;
                            }
                            let _ = socket.flush().await;
            
                            if let Err(e) = handle_connection(socket, shutdown_signal).await {
                                eprintln!("Connection error: {}", e);
                            }
                        });
                    }
                    Err(e) => {
                        eprintln!("Accept error: {}", e);
                    }
                }
            }
        }
    }

    println!("Server shutting down.");
    Ok(())
}

async fn handle_connection(mut socket: TcpStream, shutdown_signal: Arc<Notify>) -> Result<()> {
    // 1. Read command
    let mut buf = [0; 1024];
    let n = socket.read(&mut buf).await?;
    if n == 0 {
        return Ok(());
    }
    let command_line = String::from_utf8_lossy(&buf[..n]).trim().to_string();
    println!("Received command: {}", command_line);

    // Check for quit/exit command
    if command_line.eq_ignore_ascii_case("quit") || command_line.eq_ignore_ascii_case("exit") {
        println!("Quit command received. notifying shutdown.");
        shutdown_signal.notify_one();
        return Ok(());
    }

    // 2. Spawn process
    // ... rest of implementation matches previous logic
    // Detect OS for shell execution
    #[cfg(target_os = "windows")]
    let (shell, flag) = ("cmd", "/C");
    #[cfg(not(target_os = "windows"))]
    let (shell, flag) = ("sh", "-c");

    let mut child = Command::new(shell)
        .arg(flag)
        .arg(&command_line)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // .stdin(Stdio::piped()) // Future improvement for interactive
        .spawn()
        .context("Failed to spawn command")?;

    // On Windows, assign to Job Object
    #[cfg(target_os = "windows")]
    let _job_handle = {
        if let Some(handle) = child.raw_handle() {
             win_job::assign_to_new_job(handle)?
        } else {
             // Should not happen on Windows unless process already exited
             return Err(anyhow::anyhow!("Failed to get child process handle"));
        }
    };

    let stdout = child.stdout.take().context("Failed to open stdout")?;
    let stderr = child.stderr.take().context("Failed to open stderr")?;

    // 3. Stream output
    let (mut socket_reader, mut socket_writer) = socket.into_split();
    
    // Notification to kill child if socket drops
    let kill_notify = Arc::new(Notify::new());
    let kill_notify_clone_read = kill_notify.clone();
    let kill_notify_clone_write = kill_notify.clone();

    // Monitor socket for disconnection (Read EOF)
    tokio::spawn(async move {
        let mut buf = [0; 1024];
        // We don't expect any more data from client, so any read returning 0 means EOF (disconnect).
        loop {
            match socket_reader.read(&mut buf).await {
                Ok(0) => {
                    kill_notify_clone_read.notify_one();
                    break;
                }
                Ok(_) => { } // Ignore extra data
                Err(_) => {
                    kill_notify_clone_read.notify_one();
                    break;
                }
            }
        }
    });

    // Stream stdout to socket
    let mut stdout_reader = tokio::io::BufReader::new(stdout);
    let mut stderr_reader = tokio::io::BufReader::new(stderr);
    
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Vec<u8>>(32);
    let tx_stderr = tx.clone();

    let stdout_handle = tokio::spawn(async move {
        let mut buf = [0; 1024];
        loop {
            match stdout_reader.read(&mut buf).await {
                Ok(0) => break, // EOF
                Ok(n) => {
                    if tx.send(buf[..n].to_vec()).await.is_err() { break; }
                }
                Err(_) => break,
            }
        }
    });

    let stderr_handle = tokio::spawn(async move {
        let mut buf = [0; 1024];
        loop {
            match stderr_reader.read(&mut buf).await {
                Ok(0) => break, // EOF
                Ok(n) => {
                    if tx_stderr.send(buf[..n].to_vec()).await.is_err() { break; }
                }
                Err(_) => break,
            }
        }
    });

    // Write loop: receive from channel, write to socket
    let writer_handle = tokio::spawn(async move {
        while let Some(data) = rx.recv().await {
            if socket_writer.write_all(&data).await.is_err() {
                kill_notify_clone_write.notify_one();
                break;
            }
        }
        let _ = socket_writer.flush().await;
    });

    // Wait for child to exit OR kill signal
    tokio::select! {
        _ = child.wait() => {
            // Process finished normally
        }
        _ = kill_notify.notified() => {
            println!("Client disconnected, killing process...");
            let _ = child.kill().await;
        }
    }

    // Cleanup
    let _ = stdout_handle.await;
    let _ = stderr_handle.await;
    let _ = writer_handle.await;

    Ok(())
}

async fn client_mode(cmd: &str) -> Result<()> {
    // Port mapped on host: 47330 -> Container: 5330
    let addr = "127.0.0.1:47330"; 
    
    // Attempt connection loop (Connect -> Handshake -> if fail -> Bootstrap -> Retry)
    let mut attempt = 0;
    let max_attempts = 2;
    
    let mut socket = loop {
        attempt += 1;
        println!("Connecting to {} (Attempt {})...", addr, attempt);
        
        let connect_result = tokio::time::timeout(
            tokio::time::Duration::from_secs(2),
            TcpStream::connect(addr)
        ).await;

        let mut s = match connect_result {
            Ok(Ok(s)) => s,
            _ => {
                if attempt >= max_attempts {
                     return Err(anyhow::anyhow!("Failed to connect to server after bootstrap attempt"));
                }
                eprintln!("Connection failed or timed out. Bootstrapping...");
                bootstrap_server().await?;
                continue;
            }
        };

        // Handshake Check
        let mut buf = [0; 6]; // "READY\n"
        let handshake_result = tokio::time::timeout(
             tokio::time::Duration::from_millis(1000),
             s.read_exact(&mut buf)
        ).await;

        match handshake_result {
            Ok(Ok(_)) if &buf == b"READY\n" => {
                println!("Connected and verified.");
                break s;
            }
            _ => {
                 if attempt >= max_attempts {
                     return Err(anyhow::anyhow!("Handshake failed (Zombie connection?)"));
                }
                println!("Connected but no READY signal (likely Docker zombie port). Bootstrapping...");
                bootstrap_server().await?;
                continue;
            }
        }
    };

    // Send command
    socket.write_all(cmd.as_bytes()).await?;
    
    // Stream output to stdout
    let mut stdout = tokio::io::stdout();
    let mut buf = [0; 1024];
    loop {
        let n = socket.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        stdout.write_all(&buf[..n]).await?;
        stdout.flush().await?;
    }

    Ok(())
}

async fn bootstrap_server() -> Result<()> {
    let exe_path = r"C:\Users\gianca\Desktop\Shared\rust\winboat-bridge\target\x86_64-pc-windows-gnu\release\winboat-bridge.exe";
    
    // Use PowerShell Start-Process to spawn the process in a detached state.
    // -WindowStyle Hidden: Hides the window
    // -PassThru: Returns the process object (useful for debugging, though we ignore it here)
    // We direct output to files for debugging since we can't see it easily in detached mode.
    let ps_command = format!(
        "Start-Process -FilePath '{}' -ArgumentList '--server' -WindowStyle Hidden -RedirectStandardOutput 'C:\\Users\\gianca\\server.log' -RedirectStandardError 'C:\\Users\\gianca\\server.err'",
        exe_path
    );
    
    // Direct evil-winrm invocation details
    let host = "127.0.0.1";
    let port = "47320";
    let user = "gianca";
    let pass = "gianca";

    println!("Bootstrapping server via evil-winrm...");
    println!("PowerShell Command: {}", ps_command);

    // We pipe the command to evil-winrm stdin, similar to how the shell script did it.
    // This avoids complex escaping issues with passing the command as an argument to evil-winrm directly.
    let mut child = Command::new("evil-winrm")
        .arg("-i").arg(host)
        .arg("-P").arg(port)
        .arg("-u").arg(user)
        .arg("-p").arg(pass)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Failed to spawn evil-winrm")?;

    let mut stdin = child.stdin.take().context("Failed to open evil-winrm stdin")?;
    
    // Wrap the command in powershell execution
    let full_command = format!("powershell -Command \"{}\"", ps_command);
    stdin.write_all(full_command.as_bytes()).await?;
    stdin.write_all(b"\n").await?; // Add newline to execute command
    stdin.write_all(b"exit\n").await?; // Ensure shell exits
    drop(stdin); // Close stdin to signal we're done sending the command

    // Consume stdout and stderr concurrently to prevent deadlocks
    let mut stdout = child.stdout.take().context("Failed to open stdout")?;
    let mut stderr = child.stderr.take().context("Failed to open stderr")?;

    let stdout_handle = tokio::spawn(async move {
        let mut data = Vec::new();
        let _ = stdout.read_to_end(&mut data).await;
        data
    });

    let stderr_handle = tokio::spawn(async move {
        let mut data = Vec::new();
        let _ = stderr.read_to_end(&mut data).await;
        data
    });

    // Wait for evil-winrm to exit, with a timeout
    println!("Waiting for bootstrap command to complete...");
    let wait_result = tokio::time::timeout(
        tokio::time::Duration::from_secs(15),
        child.wait()
    ).await;

    match wait_result {
        Ok(Ok(status)) => {
            // Wait for I/O to finish
            let _ = stdout_handle.await; 
            let stderr_data = stderr_handle.await.unwrap_or_default();
            
             if !status.success() {
                let stderr_str = String::from_utf8_lossy(&stderr_data);
                println!("Bootstrap returned non-zero. Stderr: {}", stderr_str);
            } else {
                println!("Bootstrap command executed successfully.");
            }
        },
        Ok(Err(e)) => return Err(anyhow::anyhow!("Failed to wait for evil-winrm: {}", e)),
        Err(_) => {
            println!("Bootstrap command timed out (evil-winrm hang). Killing local process and assuming remote started.");
            let _ = child.kill().await;
        }
    }

    println!("Waiting for server to start...");
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    Ok(())
}

use anyhow::{Context as _, Result, ensure};
use rlimit::{Resource, setrlimit};
use std::collections::HashMap;
use std::process::Stdio;
use tokio::io::{self, AsyncWriteExt as _};
use tokio::process::Command;

pub async fn run_in_sandbox(command: &str, args: &[&str], stdin_bytes: &[u8]) -> Result<Vec<u8>> {
    let filtered_env: HashMap<String, String> = std::env::vars()
        .filter(|(k, _)| k == "TERM" || k == "TZ" || k == "LANG" || k == "PATH" || k == "HOME")
        .collect();

    let mut cmd = Command::new("rstrict");
    cmd.args(["--add-exec", "--ldd", "--"]);
    cmd.arg(command);
    cmd.args(args);
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    cmd.env_clear().envs(filtered_env);
    unsafe {
        cmd.pre_exec(|| {
            activate_rlimits()?;
            Ok(())
        });
    }

    let mut child = cmd.spawn()?;
    {
        let mut stdin = child.stdin.take().context("Failed to open stdin")?;
        stdin.write_all(stdin_bytes).await.context("Failed to write to stdin")?;
    }
    let output = child.wait_with_output().await?;

    ensure!(output.status.success(), String::from_utf8_lossy(&output.stderr).to_string());
    Ok(output.stdout.clone())
}

fn activate_rlimits() -> io::Result<()> {
    // CPU Time
    const CPU_LIMIT: u64 = 10; // seconds
    setrlimit(Resource::CPU, CPU_LIMIT, CPU_LIMIT)?;

    // Virtual Memory
    const AS_LIMIT: u64 = 3000 /* MiB */ * 1024 * 1024;
    setrlimit(Resource::AS, AS_LIMIT, AS_LIMIT)?;

    // File Size
    const FILE_SIZE_LIMIT: u64 = 10 /* MiB */ * 1024 * 1024;
    setrlimit(Resource::FSIZE, FILE_SIZE_LIMIT, FILE_SIZE_LIMIT)?;

    // Disable Core Dumps
    setrlimit(Resource::CORE, 0, 0)?;

    Ok(())
}

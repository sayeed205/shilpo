use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::{Child, ChildStderr, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

pub const MAX_STDERR_BYTES: usize = 64 * 1024; // 64 KiB

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessOutput {
    pub exit_code: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

pub trait ProcessRunner: Send + Sync {
    fn run_poll(
        &self,
        executable: &Path,
        args: &[String],
        cwd: &Path,
        timeout: Duration,
    ) -> Result<ProcessOutput, String>;

    fn spawn_stream(
        &self,
        executable: &Path,
        args: &[String],
        cwd: &Path,
    ) -> Result<Box<dyn StreamProcess>, String>;
}

pub trait StreamProcess: Send + Sync {
    fn next_line(&mut self, timeout: Duration) -> Result<Option<String>, String>;
    fn kill_group(&mut self) -> Result<(), String>;
    fn stderr_excerpt(&self) -> String;
}

#[derive(Default)]
pub struct RealProcessRunner;

impl ProcessRunner for RealProcessRunner {
    fn run_poll(
        &self,
        executable: &Path,
        args: &[String],
        cwd: &Path,
        timeout: Duration,
    ) -> Result<ProcessOutput, String> {
        let path_var = std::env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin".into());
        let lang_var = std::env::var("LANG").unwrap_or_else(|_| "C.UTF-8".into());
        let lcall_var = std::env::var("LC_ALL").unwrap_or_else(|_| "C.UTF-8".into());

        let mut cmd = Command::new(executable);
        cmd.args(args);
        cmd.current_dir(cwd);
        cmd.env_clear();
        cmd.env("PATH", path_var);
        cmd.env("LANG", lang_var);
        cmd.env("LC_ALL", lcall_var);
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            unsafe {
                cmd.pre_exec(|| {
                    libc::setpgid(0, 0);
                    Ok(())
                });
            }
        }

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("failed to spawn script: {e}"))?;
        let pid = child.id();

        let start = Instant::now();
        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    let mut stdout = Vec::new();
                    let mut stderr = Vec::new();
                    if let Some(mut out) = child.stdout.take() {
                        let _ = out.read_to_end(&mut stdout);
                    }
                    if let Some(mut err) = child.stderr.take() {
                        let mut buf = vec![0u8; MAX_STDERR_BYTES + 1];
                        if let Ok(n) = err.read(&mut buf) {
                            if n > MAX_STDERR_BYTES {
                                buf.truncate(MAX_STDERR_BYTES);
                                stderr = buf;
                            } else {
                                buf.truncate(n);
                                stderr = buf;
                            }
                        }
                    }
                    return Ok(ProcessOutput {
                        exit_code: status.code().unwrap_or(-1),
                        stdout,
                        stderr,
                    });
                }
                Ok(None) => {
                    if start.elapsed() >= timeout {
                        kill_and_reap_child(pid, &mut child);
                        return Err(format!("script timed out after {} ms", timeout.as_millis()));
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(e) => {
                    kill_and_reap_child(pid, &mut child);
                    return Err(format!("error waiting for child process: {e}"));
                }
            }
        }
    }

    fn spawn_stream(
        &self,
        executable: &Path,
        args: &[String],
        cwd: &Path,
    ) -> Result<Box<dyn StreamProcess>, String> {
        let path_var = std::env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin".into());
        let lang_var = std::env::var("LANG").unwrap_or_else(|_| "C.UTF-8".into());
        let lcall_var = std::env::var("LC_ALL").unwrap_or_else(|_| "C.UTF-8".into());

        let mut cmd = Command::new(executable);
        cmd.args(args);
        cmd.current_dir(cwd);
        cmd.env_clear();
        cmd.env("PATH", path_var);
        cmd.env("LANG", lang_var);
        cmd.env("LC_ALL", lcall_var);
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            unsafe {
                cmd.pre_exec(|| {
                    libc::setpgid(0, 0);
                    Ok(())
                });
            }
        }

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("failed to spawn script: {e}"))?;
        let pid = child.id();
        let stdout = child.stdout.take().ok_or("failed to open child stdout")?;
        let stderr = child.stderr.take();

        Ok(Box::new(RealStreamProcess {
            pid,
            child: Some(child),
            reader: BufReader::new(stdout),
            stderr_buf: Vec::new(),
            stderr_reader: stderr,
        }))
    }
}

pub struct RealStreamProcess {
    pid: u32,
    child: Option<Child>,
    reader: BufReader<ChildStdout>,
    stderr_buf: Vec<u8>,
    stderr_reader: Option<ChildStderr>,
}

impl StreamProcess for RealStreamProcess {
    fn next_line(&mut self, timeout: Duration) -> Result<Option<String>, String> {
        let start = Instant::now();
        let mut line = String::new();
        loop {
            match self.reader.read_line(&mut line) {
                Ok(0) => {
                    self.update_stderr();
                    return Ok(None);
                }
                Ok(_) => {
                    self.update_stderr();
                    return Ok(Some(line));
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    if start.elapsed() >= timeout {
                        return Err("stream line read timed out".into());
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(e) => {
                    self.update_stderr();
                    return Err(format!("stream read error: {e}"));
                }
            }
        }
    }

    fn kill_group(&mut self) -> Result<(), String> {
        if let Some(mut child) = self.child.take() {
            kill_and_reap_child(self.pid, &mut child);
        }
        Ok(())
    }

    fn stderr_excerpt(&self) -> String {
        String::from_utf8_lossy(&self.stderr_buf).to_string()
    }
}

impl RealStreamProcess {
    #[allow(clippy::collapsible_if)]
    fn update_stderr(&mut self) {
        if let Some(ref mut err) = self.stderr_reader {
            let mut chunk = vec![0u8; 1024];
            if let Ok(n) = err.read(&mut chunk) {
                if n > 0 {
                    let available = MAX_STDERR_BYTES.saturating_sub(self.stderr_buf.len());
                    let to_take = n.min(available);
                    self.stderr_buf.extend_from_slice(&chunk[..to_take]);
                }
            }
        }
    }
}

impl Drop for RealStreamProcess {
    fn drop(&mut self) {
        let _ = self.kill_group();
    }
}

fn kill_and_reap_child(pid: u32, child: &mut Child) {
    #[cfg(unix)]
    {
        let pgid = pid as i32;
        unsafe {
            libc::kill(-pgid, libc::SIGKILL);
        }
    }
    #[cfg(not(unix))]
    {
        let _ = child.kill();
    }
    let _ = child.wait();
}

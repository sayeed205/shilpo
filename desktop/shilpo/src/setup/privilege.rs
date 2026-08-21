use std::process::Command;

pub fn is_on_path(bin: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|dir| dir.join(bin).is_file()))
        .unwrap_or(false)
}

/// Runs `argv` with root privileges: directly if already root, otherwise via `sudo` or
/// `doas`, whichever is on `PATH`.
pub fn run_privileged(argv: &[&str]) -> Result<(), String> {
    let is_root = unsafe { libc::geteuid() } == 0;

    let mut command = if is_root {
        Command::new(argv[0])
    } else if is_on_path("sudo") {
        let mut c = Command::new("sudo");
        c.arg(argv[0]);
        c
    } else if is_on_path("doas") {
        let mut c = Command::new("doas");
        c.arg(argv[0]);
        c
    } else {
        return Err(
            "neither sudo nor doas is available, and shilpo setup is not running as root"
                .to_string(),
        );
    };
    command.args(&argv[1..]);

    let status = command
        .status()
        .map_err(|e| format!("failed to run {}: {e}", argv[0]))?;
    if !status.success() {
        return Err(format!("{} exited with a non-zero status", argv[0]));
    }
    Ok(())
}

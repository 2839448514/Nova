use std::io;
use std::process::{Command, Output};

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[cfg(target_os = "windows")]
const PWSH_PATH: &str = "C:\\Program Files\\PowerShell\\7\\pwsh.exe";

#[cfg(target_os = "windows")]
pub(crate) fn run_hidden_pwsh(command: &str) -> io::Result<Output> {
    let mut cmd = Command::new(PWSH_PATH);
    let wrapped = format!(
        "[Console]::OutputEncoding = [System.Text.Encoding]::UTF8; {}",
        command
    );
    cmd.args(["-NoLogo", "-NoProfile", "-Command", &wrapped]);
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd.output()
}

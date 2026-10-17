//! ZenProbe per-user installer / uninstaller (no admin rights needed).
//! Install:   ZenProbe-Setup.exe        (add /S for silent)
//! Uninstall: "Uninstall ZenProbe.exe"  (copied next to the app; also listed in Installed apps)

#![windows_subsystem = "windows"]

use rfd::{MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};
use std::path::{Path, PathBuf};
use std::process::Command;

const APP_EXE: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../target/release/zenprobe.exe"));
const VERSION: &str = env!("CARGO_PKG_VERSION");
const UNINSTALL_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\ZenProbe";
const UNINSTALLER: &str = "Uninstall ZenProbe.exe";

fn install_dir() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    base.join("Programs").join("ZenProbe")
}

fn ask(silent: bool, text: &str) -> bool {
    silent || MessageDialog::new().set_title("ZenProbe Setup").set_description(text).set_buttons(MessageButtons::YesNo).show() == MessageDialogResult::Yes
}

fn tell(silent: bool, level: MessageLevel, text: &str) {
    if !silent {
        MessageDialog::new().set_title("ZenProbe Setup").set_level(level).set_description(text).set_buttons(MessageButtons::Ok).show();
    }
}

/// Runs a PowerShell snippet hidden; paths go through env vars so quoting can't break.
fn powershell(script: &str, env: &[(&str, &Path)]) -> Result<(), String> {
    let mut cmd = Command::new("powershell");
    cmd.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-WindowStyle", "Hidden", "-Command", script]);
    for (k, v) in env {
        cmd.env(k, v);
    }
    hide(&mut cmd);
    let out = cmd.output().map_err(|e| e.to_string())?;
    if out.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&out.stderr).into_owned()) }
}

fn hide(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
}

fn reg(args: &[&str]) -> Result<(), String> {
    let mut cmd = Command::new("reg");
    cmd.args(args);
    hide(&mut cmd);
    let out = cmd.output().map_err(|e| e.to_string())?;
    if out.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&out.stderr).into_owned()) }
}

const SHORTCUTS: &str = r#"
$ws = New-Object -ComObject WScript.Shell
foreach ($dir in @([Environment]::GetFolderPath('Programs'), [Environment]::GetFolderPath('Desktop'))) {
  $s = $ws.CreateShortcut((Join-Path $dir 'ZenProbe.lnk'))
  $s.TargetPath = $env:ZB_EXE
  $s.WorkingDirectory = $env:ZB_DIR
  $s.Description = 'ZenProbe API client'
  $s.Save()
}
"#;

const REMOVE_SHORTCUTS: &str = r#"
foreach ($dir in @([Environment]::GetFolderPath('Programs'), [Environment]::GetFolderPath('Desktop'))) {
  Remove-Item -LiteralPath (Join-Path $dir 'ZenProbe.lnk') -Force -ErrorAction SilentlyContinue
}
"#;

fn install() -> Result<PathBuf, String> {
    let dir = install_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let exe = dir.join("ZenProbe.exe");
    // a running copy locks the file; replacing it would fail half-way
    if exe.exists() && std::fs::OpenOptions::new().write(true).open(&exe).is_err() {
        return Err("ZenProbe is running. Close it and run Setup again.".into());
    }
    std::fs::write(&exe, APP_EXE).map_err(|e| format!("cannot write {}: {e}", exe.display()))?;
    let me = std::env::current_exe().map_err(|e| e.to_string())?;
    let uninstaller = dir.join(UNINSTALLER);
    if me != uninstaller {
        std::fs::copy(&me, &uninstaller).map_err(|e| format!("cannot write uninstaller: {e}"))?;
    }
    powershell(SHORTCUTS, &[("ZB_EXE", &exe), ("ZB_DIR", &dir)]).map_err(|e| format!("shortcuts: {e}"))?;

    let exe_s = exe.to_string_lossy().to_string();
    let uninstall_cmd = format!("\"{}\" --uninstall", uninstaller.display());
    let quiet_cmd = format!("\"{}\" --uninstall /S", uninstaller.display());
    let size_kb = (APP_EXE.len() / 1024 + 1024).to_string();
    let dir_s = dir.to_string_lossy().to_string();
    for (name, kind, value) in [
        ("DisplayName", "REG_SZ", "ZenProbe"),
        ("DisplayVersion", "REG_SZ", VERSION),
        ("Publisher", "REG_SZ", "ZenProbe"),
        ("InstallLocation", "REG_SZ", dir_s.as_str()),
        ("DisplayIcon", "REG_SZ", exe_s.as_str()),
        ("UninstallString", "REG_SZ", uninstall_cmd.as_str()),
        ("QuietUninstallString", "REG_SZ", quiet_cmd.as_str()),
        ("EstimatedSize", "REG_DWORD", size_kb.as_str()),
        ("NoModify", "REG_DWORD", "1"),
        ("NoRepair", "REG_DWORD", "1"),
    ] {
        reg(&["add", UNINSTALL_KEY, "/v", name, "/t", kind, "/d", value, "/f"]).map_err(|e| format!("registry: {e}"))?;
    }
    Ok(exe)
}

fn uninstall() -> Result<(), String> {
    let dir = install_dir();
    let exe = dir.join("ZenProbe.exe");
    if exe.exists() && std::fs::OpenOptions::new().write(true).open(&exe).is_err() {
        return Err("ZenProbe is running. Close it and try again.".into());
    }
    let _ = powershell(REMOVE_SHORTCUTS, &[]);
    let _ = reg(&["delete", UNINSTALL_KEY, "/f"]);
    std::fs::remove_file(&exe).map_err(|e| e.to_string()).or_else(|e| if exe.exists() { Err(e) } else { Ok(()) })?;
    // this uninstaller can't delete itself while running: remove the folder after we exit
    let mut cmd = Command::new("cmd");
    cmd.args(["/C", "ping -n 3 127.0.0.1 >nul & rmdir /S /Q"]).arg(&dir);
    hide(&mut cmd);
    cmd.spawn().map_err(|e| e.to_string())?;
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let silent = args.iter().any(|a| a.eq_ignore_ascii_case("/S") || a == "--silent");
    let me = std::env::current_exe().unwrap_or_default();
    let uninstalling = args.iter().any(|a| a == "--uninstall") || me.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(UNINSTALLER));

    if uninstalling {
        if !ask(silent, "Remove ZenProbe from this computer?\n\nYour collections are not touched. App settings in %APPDATA%\\ZenProbe are kept.") {
            return;
        }
        match uninstall() {
            Ok(()) => tell(silent, MessageLevel::Info, "ZenProbe was removed."),
            Err(e) => {
                tell(silent, MessageLevel::Error, &format!("Uninstall failed:\n{e}"));
                std::process::exit(1);
            }
        }
        return;
    }

    let dir = install_dir();
    if !ask(silent, &format!("Install ZenProbe {VERSION}?\n\nLocation: {}\nShortcuts: Start Menu and Desktop\n\nNo administrator rights are needed.", dir.display())) {
        return;
    }
    match install() {
        Ok(exe) => {
            if !silent && ask(false, "ZenProbe was installed.\n\nLaunch it now?") {
                let _ = Command::new(&exe).current_dir(&dir).spawn();
            }
        }
        Err(e) => {
            tell(silent, MessageLevel::Error, &format!("Installation failed:\n{e}"));
            std::process::exit(1);
        }
    }
}

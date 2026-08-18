//! 앱 제거기.
//!
//! Phase 2: 정식 언인스톨러(레지스트리의 UninstallString)를 실행한다.
//!
//! 앱은 일반 권한으로 실행되므로, 관리자 권한이 필요한 언인스톨러는
//! `CreateProcess`(std::process)로는 실행되지 않는다(ERROR_ELEVATION_REQUIRED).
//! 그래서 `ShellExecuteEx`의 "runas" 동사로 실행해 필요 시 UAC 승격을 받는다.

use serde::{Deserialize, Serialize};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, ERROR_CANCELLED, HANDLE};
use windows::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject, INFINITE};
use windows::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

/// C#에서 넘어오는 제거 요청.
#[derive(Deserialize)]
pub struct UninstallRequest {
    /// 레지스트리의 UninstallString (예: `"C:\App\unins.exe" /S` 또는 `MsiExec /X{GUID}`).
    pub uninstall_string: String,
    /// 표시용 앱 이름(로깅/메시지용).
    #[serde(default)]
    pub name: String,
}

/// 제거 결과.
#[derive(Serialize)]
pub struct UninstallResult {
    pub success: bool,
    pub message: String,
    pub exit_code: Option<i32>,
}

/// UninstallString을 (실행파일, 인자)로 분리한다.
fn parse_command(cmd: &str) -> (String, String) {
    let cmd = cmd.trim();
    // 따옴표로 감싼 경로: 첫 따옴표쌍 안이 exe, 나머지가 인자.
    if let Some(rest) = cmd.strip_prefix('"') {
        if let Some(end) = rest.find('"') {
            let exe = rest[..end].to_string();
            let args = rest[end + 1..].trim().to_string();
            return (exe, args);
        }
    }
    // 따옴표 없음: 첫 ".exe" 경계에서 분리(MsiExec.exe /X... 등).
    let lower = cmd.to_lowercase();
    if let Some(pos) = lower.find(".exe") {
        let split = pos + 4;
        return (cmd[..split].to_string(), cmd[split..].trim().to_string());
    }
    (cmd.to_string(), String::new())
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// bare 실행파일명(MsiExec.exe 등)을 전체 경로로 해석한다.
/// ShellExecute("runas")는 전체 경로가 아니면 못 찾는 경우가 있어 필요.
fn resolve_exe(exe: &str) -> String {
    let e = exe.trim().trim_matches('"');
    if e.is_empty() {
        return e.to_string();
    }
    // 이미 절대경로(드라이브 또는 UNC)면 그대로.
    if e.contains(":\\") || e.starts_with("\\\\") {
        return e.to_string();
    }

    let name = if e.to_lowercase().ends_with(".exe") {
        e.to_string()
    } else {
        format!("{e}.exe")
    };

    // 1) System32 우선(MsiExec, rundll32 등 시스템 도구).
    if let Ok(sysroot) = std::env::var("SystemRoot") {
        let cand = format!("{}\\System32\\{}", sysroot.trim_end_matches('\\'), name);
        if std::path::Path::new(&cand).exists() {
            return cand;
        }
    }
    // 2) PATH 검색.
    if let Ok(path) = std::env::var("PATH") {
        for dir in path.split(';') {
            if dir.trim().is_empty() {
                continue;
            }
            let cand = format!("{}\\{}", dir.trim().trim_end_matches('\\'), name);
            if std::path::Path::new(&cand).exists() {
                return cand;
            }
        }
    }
    e.to_string()
}

/// UninstallString을 실제 실행할 (파일경로, 인자)로 변환한다.
///   - bare 실행파일명(MsiExec.exe) → 전체경로
///   - .msi 데이터파일 → `msiexec /x "<msi>"` 로 감쌈(.msi엔 runas 동사 없음)
fn build_run_command(uninstall_string: &str) -> (String, String) {
    let (exe_raw, args) = parse_command(uninstall_string.trim());
    let exe = resolve_exe(&exe_raw);
    if exe.to_lowercase().ends_with(".msi") {
        let msiexec = resolve_exe("msiexec.exe");
        let mut p = format!("/x \"{exe}\"");
        if !args.is_empty() {
            p.push(' ');
            p.push_str(&args);
        }
        (msiexec, p)
    } else {
        (exe, args)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn msi_file_wrapped_with_msiexec() {
        // 절대경로 .msi → msiexec /x "..."
        let (file, args) = build_run_command("\"C:\\Cache\\Foo\\setup.msi\"");
        assert!(file.to_lowercase().ends_with("msiexec.exe"), "file={file}");
        assert_eq!(args, "/x \"C:\\Cache\\Foo\\setup.msi\"");
    }

    #[test]
    fn msiexec_guid_resolved_full_path() {
        let (file, args) = build_run_command("MsiExec.exe /X{01567F58-B04E-49BD-B61A-AC314772F797}");
        assert!(file.to_lowercase().ends_with("msiexec.exe"));
        assert_eq!(args, "/X{01567F58-B04E-49BD-B61A-AC314772F797}");
    }

    #[test]
    fn regular_exe_unchanged() {
        let (file, args) = build_run_command("\"C:\\App\\unins.exe\" /S");
        assert_eq!(file, "C:\\App\\unins.exe");
        assert_eq!(args, "/S");
    }

    #[test]
    fn parse_msi_command() {
        let (exe, args) = parse_command("MsiExec.exe /X{01567F58-B04E-49BD-B61A-AC314772F797}");
        assert_eq!(exe, "MsiExec.exe");
        assert_eq!(args, "/X{01567F58-B04E-49BD-B61A-AC314772F797}");
    }

    #[test]
    fn parse_quoted_command() {
        let (exe, args) = parse_command("\"C:\\Program Files\\App\\unins.exe\" /S");
        assert_eq!(exe, "C:\\Program Files\\App\\unins.exe");
        assert_eq!(args, "/S");
    }

    #[test]
    fn resolve_msiexec_to_full_path() {
        let resolved = resolve_exe("MsiExec.exe").to_lowercase();
        assert!(resolved.contains(":\\"), "전체 경로로 해석돼야: {resolved}");
        assert!(resolved.ends_with("msiexec.exe"));
    }

    #[test]
    fn resolve_absolute_unchanged() {
        let p = "C:\\Program Files\\App\\unins.exe";
        assert_eq!(resolve_exe(p), p);
    }
}

/// 정식 언인스톨러를 ShellExecute("runas")로 실행하고 완료를 기다린다.
pub fn uninstall(req: &UninstallRequest) -> UninstallResult {
    let cmd_line = req.uninstall_string.trim();
    if cmd_line.is_empty() {
        return UninstallResult {
            success: false,
            message: "이 앱에는 등록된 제거 명령(UninstallString)이 없습니다.".to_owned(),
            exit_code: None,
        };
    }

    let (run_file, run_args) = build_run_command(cmd_line);
    let verb = wide("runas"); // 삭제 시 UAC 승격(삭제때만 승격 정책)
    let file = wide(&run_file);
    let params = wide(&run_args);

    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpVerb: PCWSTR(verb.as_ptr()),
        lpFile: PCWSTR(file.as_ptr()),
        lpParameters: PCWSTR(params.as_ptr()),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };

    unsafe {
        if let Err(e) = ShellExecuteExW(&mut info) {
            // 사용자가 UAC를 취소하면 ERROR_CANCELLED.
            let cancelled = windows::core::HRESULT::from_win32(ERROR_CANCELLED.0);
            let msg = if e.code() == cancelled {
                "관리자 권한 승인이 취소되어 제거를 진행하지 못했습니다.".to_owned()
            } else {
                format!("언인스톨러 실행 실패: {e}")
            };
            return UninstallResult {
                success: false,
                message: msg,
                exit_code: None,
            };
        }

        let proc: HANDLE = info.hProcess;
        if proc.is_invalid() {
            // 프로세스 핸들을 못 받으면(드물게) 실행은 된 것으로 간주.
            return UninstallResult {
                success: true,
                message: format!("'{}' 제거 프로그램을 실행했습니다.", req.name),
                exit_code: None,
            };
        }

        // 초기 프로세스가 끝날 때까지 대기.
        WaitForSingleObject(proc, INFINITE);
        let mut code: u32 = 0;
        let _ = GetExitCodeProcess(proc, &mut code);
        let _ = CloseHandle(proc);

        // ⚠️ 종료 코드로 성공/실패를 판단하지 않는다.
        // AhnLab/InstallShield 등 부트스트래퍼형은 자기 자신을 복제·재실행 후
        // 원본이 0이 아닌 코드로 즉시 종료하고, 진짜 마법사는 따로 떠 있다.
        // 실제 제거 여부는 호출 측(C#)이 "마법사 완료 후 재스캔"으로 확인한다.
        UninstallResult {
            success: true,
            message: format!("'{}' 제거 프로그램을 실행했습니다.", req.name),
            exit_code: Some(code as i32),
        }
    }
}

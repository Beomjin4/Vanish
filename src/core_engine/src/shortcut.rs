//! 바로가기(.lnk) → 설치 앱 매칭 (F3, 드래그앤드롭).
//!
//! C#이 .lnk에서 대상 exe 경로를 추출해 넘겨주면(WScript.Shell COM),
//! 여기서 설치 앱 목록과 대조해 어떤 앱인지 찾아낸다.

use crate::scanner::{self, AppInfo};

/// 대상 exe 경로로 설치 앱을 찾는다. 매칭 실패 시 None.
///
/// 전략(우선순위):
///   1. InstallLocation이 대상 경로의 상위 폴더인 앱 (가장 긴 일치 우선)
///   2. DisplayIcon이 가리키는 exe와 대상 exe가 같은 앱
pub fn match_shortcut(target_exe: &str) -> Option<AppInfo> {
    let target = target_exe.trim().to_lowercase().replace('/', "\\");
    if target.is_empty() {
        return None;
    }

    let apps = scanner::scan_installed_apps();

    // 각 앱의 "기준 폴더" 후보: InstallLocation, 그리고 DisplayIcon의 상위 폴더.
    // 대상 exe가 이 폴더 아래(접두 일치)면 매칭. 가장 구체적인(긴) 경로 우선.
    let mut best: Option<(usize, AppInfo)> = None;
    for app in &apps {
        for dir in app_base_dirs(app) {
            if !dir.is_empty() && target.starts_with(&dir) {
                let len = dir.len();
                if best.as_ref().map_or(true, |(blen, _)| len > *blen) {
                    best = Some((len, app.clone()));
                }
            }
        }
    }
    if let Some((_, app)) = best {
        return Some(app);
    }

    // 마지막 폴백: DisplayIcon exe 경로가 대상과 정확히 같은 경우.
    for app in &apps {
        if let Some(exe) = display_icon_path(app) {
            if exe == target {
                return Some(app.clone());
            }
        }
    }

    None
}

/// 앱의 기준 폴더 후보들(소문자, 끝 역슬래시 제거, 시스템 경로 제외).
fn app_base_dirs(app: &AppInfo) -> Vec<String> {
    let mut dirs = Vec::new();

    // 1) InstallLocation 자체가 폴더.
    let loc = app.install_location.trim();
    if !loc.is_empty() {
        dirs.push(normalize_dir(loc));
    }
    // 2) DisplayIcon / UninstallString이 가리키는 파일의 상위 폴더
    //    (대개 설치 폴더 안의 exe/ico/언인스톨러).
    if let Some(dir) = parent_dir_of(&app.icon) {
        dirs.push(dir);
    }
    if let Some(dir) = parent_dir_of(&app.uninstall_string) {
        dirs.push(dir);
    }

    // 시스템/공유 폴더는 기준 폴더로 부적합 → 제외.
    dirs.retain(|d| !d.is_empty() && !is_system_dir(d));
    dirs.sort();
    dirs.dedup();
    dirs
}

/// DisplayIcon exe 경로만(정확 일치 폴백용). exe가 아니면 None.
fn display_icon_path(app: &AppInfo) -> Option<String> {
    let path = first_path_token(&app.icon)?;
    if path.ends_with(".exe") {
        Some(path)
    } else {
        None
    }
}

/// "path,index" / 따옴표 포함 문자열에서 첫 경로 토큰을 정규화해 뽑는다.
fn first_path_token(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    // 따옴표로 감싼 경우 첫 따옴표쌍 안을, 아니면 콤마 앞을 사용.
    let path = if raw.starts_with('"') {
        raw[1..].split('"').next().unwrap_or("").to_string()
    } else {
        raw.split(',').next().unwrap_or("").to_string()
    };
    let path = path.trim().to_lowercase().replace('/', "\\");
    if path.is_empty() {
        None
    } else {
        Some(path)
    }
}

/// 경로 문자열에서 상위 폴더를 뽑는다.
fn parent_dir_of(raw: &str) -> Option<String> {
    let path = first_path_token(raw)?;
    let idx = path.rfind('\\')?;
    let dir = path[..idx].trim_end_matches('\\').to_string();
    if dir.is_empty() {
        None
    } else {
        Some(dir)
    }
}

/// 시스템/공유 폴더 여부(기준 폴더로 쓰면 오탐 위험).
fn is_system_dir(dir: &str) -> bool {
    const SYS: &[&str] = &[
        r"\windows",
        r"\system32",
        r"\program files\common files",
        r"\program files (x86)\common files",
    ];
    SYS.iter().any(|s| dir.contains(s))
        // "c:\program files" 처럼 너무 얕은 경로(설치 루트 자체)도 제외.
        || dir.matches('\\').count() <= 1
}

fn normalize_dir(p: &str) -> String {
    p.to_lowercase()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_string()
}

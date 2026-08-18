//! 설치 앱 스캐너.
//!
//! Windows는 설치된 프로그램 정보를 레지스트리의 "Uninstall" 키 아래에 둔다.
//! 명시적 경로 3곳을 직접 조회한다(WOW64 리다이렉션 플래그를 쓰지 않아 단순·명확):
//!   - HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall            (64비트 앱)
//!   - HKLM\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall (32비트 앱)
//!   - HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall            (사용자별 앱)

use serde::Serialize;
use winreg::enums::*;
use winreg::{RegKey, HKEY};

const UNINSTALL: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall";
const UNINSTALL_WOW: &str = r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall";

/// C#으로 전달할 설치 앱 1건의 정보.
#[derive(Serialize, Clone, Debug)]
pub struct AppInfo {
    /// 레지스트리 하위 키 이름(고유 식별자로 사용).
    pub id: String,
    pub name: String,
    pub publisher: String,
    pub version: String,
    pub install_location: String,
    pub uninstall_string: String,
    pub icon: String,
    /// 추정 용량(MB). 모르면 0.
    pub size_mb: u64,
    /// 설치일(YYYYMMDD 문자열, 모르면 빈 문자열).
    pub install_date: String,
    /// 어느 레지스트리 뷰에서 왔는지: "HKLM64" | "HKLM32" | "HKCU".
    pub source: String,
}

/// 한 레지스트리 위치의 Uninstall 키 아래를 훑어 앱 목록을 모은다.
fn scan_hive(hive: HKEY, path: &str, source: &str, out: &mut Vec<AppInfo>) {
    let root = RegKey::predef(hive);
    let uninstall = match root.open_subkey_with_flags(path, KEY_READ) {
        Ok(k) => k,
        Err(_) => return, // 해당 키가 없으면(예: WOW6432Node 없는 환경) 조용히 건너뜀
    };

    for sub_name in uninstall.enum_keys().flatten() {
        let app = match uninstall.open_subkey(&sub_name) {
            Ok(k) => k,
            Err(_) => continue,
        };

        // DisplayName이 없으면 사용자에게 보여줄 게 없으므로 제외.
        let name: String = match app.get_value("DisplayName") {
            Ok(n) => n,
            Err(_) => continue,
        };

        // 시스템 구성요소/업데이트(KB)는 일반 앱이 아니므로 숨긴다.
        let system_component: u32 = app.get_value("SystemComponent").unwrap_or(0);
        if system_component == 1 {
            continue;
        }
        // 윈도우 업데이트 항목(부모 키가 있는 패치)도 제외.
        let release_type: String = app.get_value("ReleaseType").unwrap_or_default();
        if release_type == "Security Update" || release_type == "Update Rollup" || release_type == "Hotfix" {
            continue;
        }

        let size_kb: u32 = app.get_value("EstimatedSize").unwrap_or(0);

        out.push(AppInfo {
            id: sub_name.clone(),
            name,
            publisher: app.get_value("Publisher").unwrap_or_default(),
            version: app.get_value("DisplayVersion").unwrap_or_default(),
            install_location: app.get_value("InstallLocation").unwrap_or_default(),
            // 일반 UninstallString을 우선 사용한다(Windows '설정'과 동일하게 확인창 표시).
            // 비어 있을 때만 QuietUninstallString로 폴백.
            uninstall_string: app
                .get_value("UninstallString")
                .ok()
                .filter(|s: &String| !s.trim().is_empty())
                .or_else(|| app.get_value("QuietUninstallString").ok())
                .unwrap_or_default(),
            icon: app.get_value("DisplayIcon").unwrap_or_default(),
            size_mb: (size_kb as u64) / 1024,
            install_date: app.get_value("InstallDate").unwrap_or_default(),
            source: source.to_string(),
        });
    }
}

/// 세 레지스트리 위치를 모두 스캔하고, 중복(같은 이름+버전)을 제거해 반환한다.
pub fn scan_installed_apps() -> Vec<AppInfo> {
    let mut apps = Vec::new();

    scan_hive(HKEY_LOCAL_MACHINE, UNINSTALL, "HKLM64", &mut apps);
    scan_hive(HKEY_LOCAL_MACHINE, UNINSTALL_WOW, "HKLM32", &mut apps);
    scan_hive(HKEY_CURRENT_USER, UNINSTALL, "HKCU", &mut apps);

    // 이름+버전이 동일한 항목은 한 번만 남긴다(64/32 뷰에 중복 등록되는 경우 대비).
    apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    apps.dedup_by(|a, b| a.name == b.name && a.version == b.version);

    apps
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_returns_some_apps() {
        // 어떤 윈도우 머신이든 설치 앱이 최소 몇 개는 있으므로 비어있지 않아야 한다.
        let apps = scan_installed_apps();
        assert!(!apps.is_empty(), "설치된 앱이 하나도 잡히지 않음");
        // 이름이 빈 항목은 없어야 한다.
        assert!(apps.iter().all(|a| !a.name.is_empty()));
    }
}

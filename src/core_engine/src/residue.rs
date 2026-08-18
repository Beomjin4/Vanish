//! 잔여물(residue) 탐색·삭제.
//!
//! 정식 언인스톨러가 지우지 못하고 남긴 파일/폴더와 레지스트리 키를 찾는다.
//!
//! ⚠️ 가장 위험한 모듈이다. 오탐(false positive)으로 무관한 폴더를 지우면
//! 사용자 데이터가 사라진다. 따라서 다음 원칙을 지킨다:
//!   1. 절대 자동 삭제하지 않는다. 후보를 찾아 C#에 돌려주고, 사용자가 체크한 것만 지운다.
//!   2. 시스템/공유 경로는 화이트리스트로 보호한다(절대 후보에 넣지 않음).
//!   3. 파일/폴더는 영구삭제가 아니라 휴지통으로 보낸다(복구 가능).

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use winreg::enums::*;
use winreg::RegKey;

/// 잔여물 후보 1건.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ResidueItem {
    /// 파일/폴더 경로 또는 레지스트리 경로(예: `HKCU\Software\Foo`).
    pub path: String,
    /// "folder" | "registry"
    pub kind: String,
    /// 폴더 용량(MB). 레지스트리는 0.
    pub size_mb: u64,
}

/// 잔여물 탐색 요청(C#에서 전달).
#[derive(Deserialize)]
pub struct FindRequest {
    pub name: String,
    #[serde(default)]
    pub publisher: String,
    #[serde(default)]
    pub install_location: String,
}

/// 잔여물 삭제 요청.
#[derive(Deserialize)]
pub struct DeleteRequest {
    pub items: Vec<ResidueItem>,
}

/// 삭제 결과.
#[derive(Serialize)]
pub struct DeleteResult {
    pub deleted: Vec<String>,
    pub failed: Vec<FailedItem>,
    /// 휴지통으로 보낸 파일/폴더 경로(이후 영구삭제(F6)용 추적).
    pub recycled_paths: Vec<String>,
}

#[derive(Serialize)]
pub struct FailedItem {
    pub path: String,
    pub error: String,
}

/// 이 경로/이름은 절대 잔여물 후보로 삼지 않는다(시스템·공유 폴더 보호).
fn is_protected(path: &Path) -> bool {
    let s = path.to_string_lossy().to_lowercase();
    // %LocalAppData%\Packages 루트 자체는 보호하되,
    // 그 아래 개별 패키지 폴더(MSIX 앱 잔여물)는 삭제 허용.
    if s.ends_with(r"\appdata\local\packages") {
        return true;
    }
    const PROTECTED: &[&str] = &[
        r"\windows",
        r"\program files\common files",
        r"\program files (x86)\common files",
        r"\program files\windowsapps",
        r"\microsoft\windows",
    ];
    if PROTECTED.iter().any(|p| s.contains(p)) {
        return true;
    }
    // 너무 얕은 경로(루트나 드라이브 바로 아래 단일 폴더)는 보호.
    path.components().count() <= 3
}

/// 후보 폴더명 매칭에 쓸 토큰들(소문자, 4글자 이상).
fn match_tokens(name: &str, publisher: &str) -> Vec<String> {
    let mut tokens = Vec::new();

    // 너무 흔해서 단독으로 매칭하면 오탐을 부르는 일반 단어.
    // 예: "OBS Studio"의 'studio'가 'Visual Studio Setup' 폴더를 잘못 매칭.
    const GENERIC: &[&str] = &[
        "studio", "setup", "install", "installer", "uninstall", "update", "updater",
        "launcher", "manager", "client", "server", "desktop", "browser", "player",
        "viewer", "editor", "reader", "tool", "tools", "service", "services", "helper",
        "system", "application", "applications", "software", "program", "programs",
        "edition", "version", "project", "projects", "files", "data", "cache", "temp",
        "common", "shared", "office", "home", "free", "pro", "plus", "lite", "full",
        "windows", "microsoft", "google", "intel", "nvidia", "amd", "driver", "drivers",
        "runtime", "framework", "package", "packages", "core", "main", "default",
    ];
    let is_generic = |w: &str| GENERIC.contains(&w);

    // 앱 이름에서 버전/숫자/특수문자를 떼어내고 의미있는 단어만.
    let cleaned: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect();
    // 공백 제거한 압축 풀네임(가장 구별력 높음). 일반 단어와 같으면 제외.
    let compact: String = cleaned
        .chars()
        .filter(|c| c.is_alphabetic())
        .collect::<String>()
        .to_lowercase();
    if compact.len() >= 4 && !is_generic(&compact) {
        tokens.push(compact);
    }
    // 개별 단어: 4글자 이상 + 일반 단어 아님.
    for word in cleaned.split_whitespace() {
        let w = word.to_lowercase();
        if w.len() >= 4 && w.chars().any(|c| c.is_alphabetic()) && !is_generic(&w) {
            tokens.push(w);
        }
    }

    // 제조사도 토큰으로(법인 접미사·일반 단어 제거).
    let pub_clean: String = publisher
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect();
    for word in pub_clean.split_whitespace() {
        let w = word.to_lowercase();
        const STOP: &[&str] = &["inc", "ltd", "llc", "corp", "corporation", "company", "co", "the", "technologies", "limited", "gmbh"];
        if w.len() >= 4 && !STOP.contains(&w.as_str()) && !is_generic(&w) {
            tokens.push(w);
        }
    }

    tokens.sort();
    tokens.dedup();
    tokens
}

/// 폴더명이 토큰 중 하나와 매칭되는지(대소문자 무시, 부분일치).
fn folder_matches(folder_name: &str, tokens: &[String]) -> bool {
    let fname = folder_name.to_lowercase();
    let compact: String = fname.chars().filter(|c| c.is_alphanumeric()).collect();
    tokens.iter().any(|t| compact.contains(t.as_str()) || fname.contains(t.as_str()))
}

/// 환경변수 기반 사용자 데이터 루트들.
fn data_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    for var in ["APPDATA", "LOCALAPPDATA", "PROGRAMDATA"] {
        if let Ok(v) = std::env::var(var) {
            roots.push(PathBuf::from(v));
        }
    }
    // LocalAppData\Programs (사용자별 설치 앱이 흔히 쓰는 위치)
    if let Ok(v) = std::env::var("LOCALAPPDATA") {
        roots.push(PathBuf::from(v).join("Programs"));
    }
    roots
}

/// 폴더 용량(MB)을 대략 계산(상한을 둬서 거대한 트리 스캔 방지).
fn dir_size_mb(path: &Path) -> u64 {
    fn walk(path: &Path, acc: &mut u64, budget: &mut u32) {
        if *budget == 0 {
            return;
        }
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                *budget = budget.saturating_sub(1);
                if *budget == 0 {
                    return;
                }
                let p = entry.path();
                if let Ok(meta) = entry.metadata() {
                    if meta.is_dir() {
                        walk(&p, acc, budget);
                    } else {
                        *acc += meta.len();
                    }
                }
            }
        }
    }
    let mut bytes = 0u64;
    let mut budget = 20_000u32; // 최대 2만 엔트리까지만.
    walk(path, &mut bytes, &mut budget);
    bytes / (1024 * 1024)
}

/// 잔여물 후보를 찾는다.
pub fn find_residue(req: &FindRequest) -> Vec<ResidueItem> {
    let tokens = match_tokens(&req.name, &req.publisher);
    let mut items = Vec::new();
    let mut seen = std::collections::HashSet::new();

    // 1) 데이터 루트의 '바로 아래' 폴더만 검사(깊이 제한으로 오탐·비용 절감).
    if !tokens.is_empty() {
        for root in data_roots() {
            if let Ok(entries) = std::fs::read_dir(&root) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if !p.is_dir() || is_protected(&p) {
                        continue;
                    }
                    let fname = entry.file_name().to_string_lossy().into_owned();
                    if folder_matches(&fname, &tokens) && seen.insert(p.clone()) {
                        items.push(ResidueItem {
                            size_mb: dir_size_mb(&p),
                            path: p.to_string_lossy().into_owned(),
                            kind: "folder".to_owned(),
                        });
                    }
                }
            }
        }
    }

    // 2) 설치 폴더가 제거 후에도 남아 있으면 후보로.
    if !req.install_location.is_empty() {
        let p = PathBuf::from(&req.install_location);
        if p.is_dir() && !is_protected(&p) && seen.insert(p.clone()) {
            items.push(ResidueItem {
                size_mb: dir_size_mb(&p),
                path: p.to_string_lossy().into_owned(),
                kind: "folder".to_owned(),
            });
        }
    }

    // 3) 레지스트리 잔여 키(제조사/이름 기준, 존재하는 것만).
    items.extend(find_registry_residue(req));

    items
}

/// HKCU/HKLM Software 아래 제조사·이름 키가 남아 있으면 후보로.
fn find_registry_residue(req: &FindRequest) -> Vec<ResidueItem> {
    let mut out = Vec::new();
    let mut candidates: Vec<String> = Vec::new();
    if !req.publisher.trim().is_empty() {
        candidates.push(req.publisher.trim().to_owned());
    }
    if !req.name.trim().is_empty() {
        candidates.push(req.name.trim().to_owned());
    }

    let hives = [
        (HKEY_CURRENT_USER, "HKCU", r"Software"),
        (HKEY_LOCAL_MACHINE, "HKLM", r"Software"),
        (HKEY_LOCAL_MACHINE, "HKLM", r"Software\WOW6432Node"),
    ];

    for (hive, label, base) in hives {
        let root = RegKey::predef(hive);
        for cand in &candidates {
            let subpath = format!(r"{base}\{cand}");
            if root.open_subkey(&subpath).is_ok() {
                out.push(ResidueItem {
                    path: format!(r"{label}\{subpath}"),
                    kind: "registry".to_owned(),
                    size_mb: 0,
                });
            }
        }
    }
    out
}

/// 사용자가 체크한 잔여물만 삭제한다. 폴더는 휴지통으로, 레지스트리 키는 삭제.
pub fn delete_residue(req: &DeleteRequest) -> DeleteResult {
    let mut result = DeleteResult {
        deleted: Vec::new(),
        failed: Vec::new(),
        recycled_paths: Vec::new(),
    };

    for item in &req.items {
        match item.kind.as_str() {
            "folder" => {
                let p = Path::new(&item.path);
                // 이미 없는 폴더(예: MSIX 제거 시 Windows가 함께 정리)는 성공으로 처리.
                if !p.exists() {
                    result.deleted.push(item.path.clone());
                    continue;
                }
                // 안전장치: 보호 경로는 무조건 거부(요청이 와도).
                if is_protected(p) {
                    result.failed.push(FailedItem {
                        path: item.path.clone(),
                        error: "보호된 시스템 경로라 삭제할 수 없습니다.".to_owned(),
                    });
                    continue;
                }
                match trash::delete(p) {
                    Ok(_) => {
                        result.recycled_paths.push(item.path.clone());
                        result.deleted.push(item.path.clone());
                    }
                    Err(e) => result.failed.push(FailedItem {
                        path: item.path.clone(),
                        error: format!("휴지통 이동 실패: {e}"),
                    }),
                }
            }
            "registry" => match delete_registry_key(&item.path) {
                Ok(_) => result.deleted.push(item.path.clone()),
                Err(e) => result.failed.push(FailedItem {
                    path: item.path.clone(),
                    error: e,
                }),
            },
            other => result.failed.push(FailedItem {
                path: item.path.clone(),
                error: format!("알 수 없는 항목 종류: {other}"),
            }),
        }
    }

    result
}

/// `HKCU\Software\Foo` 형태 경로의 레지스트리 키를 (하위 포함) 삭제.
fn delete_registry_key(full_path: &str) -> Result<(), String> {
    let (label, sub) = full_path
        .split_once('\\')
        .ok_or_else(|| "잘못된 레지스트리 경로".to_owned())?;
    let hive = match label {
        "HKCU" => HKEY_CURRENT_USER,
        "HKLM" => HKEY_LOCAL_MACHINE,
        _ => return Err(format!("지원하지 않는 하이브: {label}")),
    };
    let root = RegKey::predef(hive);
    root.delete_subkey_all(sub)
        .map_err(|e| format!("레지스트리 키 삭제 실패: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protected_paths_blocked() {
        assert!(is_protected(Path::new(r"C:\Windows\System32")));
        assert!(is_protected(Path::new(r"C:\Program Files\Common Files\foo")));
        assert!(is_protected(Path::new(r"C:\")));
        assert!(!is_protected(Path::new(r"C:\Users\me\AppData\Roaming\MyApp")));
        // Packages 루트는 보호, 개별 패키지 폴더(MSIX 잔여물)는 허용.
        assert!(is_protected(Path::new(r"C:\Users\me\AppData\Local\Packages")));
        assert!(!is_protected(Path::new(
            r"C:\Users\me\AppData\Local\Packages\CanonicalGroupLimited.Ubuntu_79rhkp1fndgsc"
        )));
    }

    #[test]
    fn tokens_skip_versions_and_stopwords() {
        let t = match_tokens("Arduino IDE 2.3.8", "Arduino SA");
        assert!(t.iter().any(|x| x == "arduino"));
        // 버전 숫자나 짧은 단어는 토큰에 없어야.
        assert!(!t.iter().any(|x| x == "2" || x == "238"));
    }

    #[test]
    fn folder_match_basic() {
        let tokens = match_tokens("Adobe Photoshop 2026", "Adobe Inc.");
        assert!(folder_matches("Adobe", &tokens));
        assert!(folder_matches("Photoshop", &tokens));
        assert!(!folder_matches("Steam", &tokens));
    }

    #[test]
    fn generic_word_no_false_positive() {
        // "OBS Studio" → 'studio' 일반 단어 제외, 'obsstudio' 압축형만.
        let tokens = match_tokens("OBS Studio", "OBS Project");
        assert!(tokens.iter().any(|t| t == "obsstudio"));
        assert!(!tokens.iter().any(|t| t == "studio"));
        // 'Visual Studio Setup' 폴더를 잘못 매칭하면 안 된다.
        assert!(!folder_matches("Visual Studio Setup", &tokens));
        // 실제 OBS 폴더는 매칭돼야 한다.
        assert!(folder_matches("obs-studio", &tokens));
    }
}

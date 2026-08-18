//! 백업/복원 (Phase 7).
//!
//! 앱의 데이터/설정 폴더(%AppData% 등)를 하나의 `.appbak` 아카이브로 묶는다.
//! 아카이브 = tar(여러 폴더 묶음) + zstd(압축). 내부 구조:
//!   - `manifest.json` : 앱 정보 + 각 항목의 원본 루트(APPDATA/LOCALAPPDATA/...) 매핑
//!   - `e0/...`, `e1/...` : 항목별(폴더) 파일들
//!
//! 다른 PC에서 복원하면 manifest의 root_kind를 그 PC의 환경변수로 풀어
//! 같은 상대 위치에 되돌린다(기기 간 경로 차이 흡수).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use winreg::enums::*;
use winreg::{RegKey, RegValue};

/// 백업 항목 1개(폴더)의 위치 정보.
#[derive(Serialize, Deserialize, Clone)]
pub struct BackupEntry {
    /// "APPDATA" | "LOCALAPPDATA" | "PROGRAMDATA" | "ABSOLUTE"
    pub root_kind: String,
    /// root 기준 상대경로(ABSOLUTE면 전체 경로). 슬래시는 '\\' 유지.
    pub relative_path: String,
    /// 표시용 원본 전체 경로.
    pub original_path: String,
}

/// 백업된 레지스트리 키 1개.
#[derive(Serialize, Deserialize, Clone)]
pub struct RegEntry {
    /// 예: `HKCU\Software\Foo`
    pub key_path: String,
    /// 아카이브 내 JSON 파일명(예: `reg/0.json`).
    pub archive_name: String,
}

/// 레지스트리 값 1개(타입 + 원시 바이트).
#[derive(Serialize, Deserialize, Clone)]
pub struct RegValueData {
    pub vtype: u32,
    pub data: Vec<u8>,
}

/// 레지스트리 키 트리(값 + 하위키 재귀).
#[derive(Serialize, Deserialize, Clone, Default)]
pub struct RegNode {
    pub values: HashMap<String, RegValueData>,
    pub subkeys: HashMap<String, RegNode>,
}

/// 아카이브에 담기는 메타데이터.
#[derive(Serialize, Deserialize, Clone)]
pub struct Manifest {
    pub format: String, // "appcleaner-backup-v1"
    pub name: String,
    pub publisher: String,
    pub version: String,
    pub source_machine: String,
    pub created_at: String,
    pub entries: Vec<BackupEntry>,
    #[serde(default)]
    pub registry: Vec<RegEntry>,
}

#[derive(Deserialize)]
pub struct CreateRequest {
    pub name: String,
    #[serde(default)]
    pub publisher: String,
    #[serde(default)]
    pub version: String,
    /// 백업할 폴더들의 절대경로.
    pub paths: Vec<String>,
    /// 백업할 레지스트리 키들(예: `HKCU\Software\Foo`).
    #[serde(default)]
    pub registry_keys: Vec<String>,
    /// 출력 .appbak 경로.
    pub output_path: String,
}

#[derive(Serialize)]
pub struct CreateResult {
    pub success: bool,
    pub message: String,
    pub entry_count: usize,
}

#[derive(Deserialize)]
pub struct RestoreRequest {
    pub archive_path: String,
    /// true면 기존 파일 덮어쓰기.
    #[serde(default)]
    pub overwrite: bool,
}

#[derive(Serialize)]
pub struct RestoreResult {
    pub success: bool,
    pub message: String,
    pub restored_entries: usize,
}

/// 알려진 데이터 루트(환경변수)와 라벨.
fn known_roots() -> Vec<(String, String)> {
    let mut v = Vec::new();
    for key in ["APPDATA", "LOCALAPPDATA", "PROGRAMDATA"] {
        if let Ok(val) = std::env::var(key) {
            v.push((key.to_string(), val));
        }
    }
    v
}

/// 절대경로를 (root_kind, relative_path)로 분류한다.
fn classify(path: &str) -> (String, String) {
    let lower = path.to_lowercase();
    for (kind, root) in known_roots() {
        let root_l = root.to_lowercase();
        if lower.starts_with(&root_l) {
            let rel = path[root.len()..].trim_start_matches(['\\', '/']).to_string();
            return (kind, rel);
        }
    }
    ("ABSOLUTE".to_string(), path.to_string())
}

/// root_kind를 이 PC의 실제 경로로 푼다(ABSOLUTE면 빈 문자열).
fn resolve_root(kind: &str) -> Option<String> {
    match kind {
        "ABSOLUTE" => Some(String::new()),
        other => std::env::var(other).ok(),
    }
}

/// `HKCU\Software\Foo` 형태에서 (하이브, 하위경로)를 얻는다.
fn split_hive(full: &str) -> Option<(winreg::HKEY, &str)> {
    let (label, sub) = full.split_once('\\')?;
    let hive = match label {
        "HKCU" => HKEY_CURRENT_USER,
        "HKLM" => HKEY_LOCAL_MACHINE,
        _ => return None,
    };
    Some((hive, sub))
}

/// 레지스트리 키 트리를 재귀적으로 읽어 RegNode로 만든다.
fn export_node(key: &RegKey) -> RegNode {
    let mut node = RegNode::default();
    for item in key.enum_values().flatten() {
        let (name, val) = item;
        node.values.insert(
            name,
            RegValueData {
                vtype: val.vtype.clone() as u32,
                data: val.bytes,
            },
        );
    }
    for name in key.enum_keys().flatten() {
        if let Ok(sub) = key.open_subkey(&name) {
            node.subkeys.insert(name, export_node(&sub));
        }
    }
    node
}

/// 레지스트리 키를 백업용으로 export. 없으면 None.
fn export_key(full_path: &str) -> Option<RegNode> {
    let (hive, sub) = split_hive(full_path)?;
    let key = RegKey::predef(hive).open_subkey(sub).ok()?;
    Some(export_node(&key))
}

/// u32 → winreg RegType (raw value 복원용).
fn u32_to_regtype(v: u32) -> winreg::enums::RegType {
    use winreg::enums::RegType::*;
    match v {
        0 => REG_NONE,
        1 => REG_SZ,
        2 => REG_EXPAND_SZ,
        3 => REG_BINARY,
        4 => REG_DWORD,
        5 => REG_DWORD_BIG_ENDIAN,
        6 => REG_LINK,
        7 => REG_MULTI_SZ,
        8 => REG_RESOURCE_LIST,
        9 => REG_FULL_RESOURCE_DESCRIPTOR,
        10 => REG_RESOURCE_REQUIREMENTS_LIST,
        11 => REG_QWORD,
        _ => REG_BINARY,
    }
}

/// RegNode를 지정 키 아래에 재귀적으로 기록한다.
fn import_node(key: &RegKey, node: &RegNode) -> std::io::Result<()> {
    for (name, v) in &node.values {
        let rv = RegValue {
            bytes: v.data.clone(),
            vtype: u32_to_regtype(v.vtype),
        };
        key.set_raw_value(name, &rv)?;
    }
    for (name, sub) in &node.subkeys {
        let (subkey, _) = key.create_subkey(name)?;
        import_node(&subkey, sub)?;
    }
    Ok(())
}

/// 백업된 레지스트리 키를 이 PC에 복원한다.
fn import_key(full_path: &str, node: &RegNode) -> std::io::Result<()> {
    let (hive, sub) = split_hive(full_path)
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "잘못된 키 경로"))?;
    let (key, _) = RegKey::predef(hive).create_subkey(sub)?;
    import_node(&key, node)
}

/// `.appbak` 아카이브를 만든다.
pub fn create_backup(req: &CreateRequest) -> CreateResult {
    let mut entries = Vec::new();
    for p in &req.paths {
        let path = Path::new(p);
        if !path.exists() {
            continue;
        }
        let (root_kind, relative_path) = classify(p);
        entries.push(BackupEntry {
            root_kind,
            relative_path,
            original_path: p.clone(),
        });
    }

    // 레지스트리 키 export.
    let mut reg_nodes: Vec<(RegEntry, RegNode)> = Vec::new();
    for (i, key_path) in req.registry_keys.iter().enumerate() {
        if let Some(node) = export_key(key_path) {
            reg_nodes.push((
                RegEntry {
                    key_path: key_path.clone(),
                    archive_name: format!("reg/{i}.json"),
                },
                node,
            ));
        }
    }

    if entries.is_empty() && reg_nodes.is_empty() {
        return CreateResult {
            success: false,
            message: "백업할 유효한 폴더/레지스트리가 없습니다.".to_owned(),
            entry_count: 0,
        };
    }

    let manifest = Manifest {
        format: "appcleaner-backup-v1".to_owned(),
        name: req.name.clone(),
        publisher: req.publisher.clone(),
        version: req.version.clone(),
        source_machine: std::env::var("COMPUTERNAME").unwrap_or_default(),
        created_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        entries: entries.clone(),
        registry: reg_nodes.iter().map(|(e, _)| e.clone()).collect(),
    };

    let total = entries.len() + reg_nodes.len();
    match write_archive(&req.output_path, &manifest, &entries, &reg_nodes) {
        Ok(_) => CreateResult {
            success: true,
            message: format!("{}개 항목(폴더 {}, 레지스트리 {})을 백업했습니다.", total, entries.len(), reg_nodes.len()),
            entry_count: total,
        },
        Err(e) => CreateResult {
            success: false,
            message: format!("백업 생성 실패: {e}"),
            entry_count: 0,
        },
    }
}

fn append_bytes(tar: &mut tar::Builder<impl std::io::Write>, name: &str, bytes: &[u8]) -> std::io::Result<()> {
    let mut header = tar::Header::new_gnu();
    header.set_size(bytes.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    tar.append_data(&mut header, name, bytes)
}

fn write_archive(
    output: &str,
    manifest: &Manifest,
    entries: &[BackupEntry],
    reg_nodes: &[(RegEntry, RegNode)],
) -> std::io::Result<()> {
    let file = File::create(output)?;
    // zstd 레벨 10(속도/압축 균형). auto_finish로 drop 시 마무리.
    let encoder = zstd::Encoder::new(file, 10)?.auto_finish();
    let mut tar = tar::Builder::new(encoder);

    // manifest.json
    let mbytes = serde_json::to_vec_pretty(manifest)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    append_bytes(&mut tar, "manifest.json", &mbytes)?;

    // 각 폴더를 e{i}/ 아래에 통째로.
    for (i, entry) in entries.iter().enumerate() {
        let src = Path::new(&entry.original_path);
        if src.is_dir() {
            tar.append_dir_all(format!("e{i}"), src)?;
        } else if src.is_file() {
            let name = format!("e{i}/{}", src.file_name().unwrap_or_default().to_string_lossy());
            tar.append_path_with_name(src, name)?;
        }
    }

    // 레지스트리 노드들을 reg/{i}.json 으로.
    for (entry, node) in reg_nodes {
        let bytes = serde_json::to_vec(node)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        append_bytes(&mut tar, &entry.archive_name, &bytes)?;
    }

    tar.finish()?;
    Ok(())
}

/// 아카이브에서 manifest만 읽어 반환(복원 전 미리보기용).
pub fn inspect_backup(archive_path: &str) -> Result<Manifest, String> {
    let file = File::open(archive_path).map_err(|e| format!("열기 실패: {e}"))?;
    let decoder = zstd::Decoder::new(file).map_err(|e| format!("압축 해제 실패: {e}"))?;
    let mut archive = tar::Archive::new(decoder);

    for entry in archive.entries().map_err(|e| format!("아카이브 읽기 실패: {e}"))? {
        let mut entry = entry.map_err(|e| format!("항목 읽기 실패: {e}"))?;
        let path = entry
            .path()
            .map_err(|e| format!("경로 읽기 실패: {e}"))?
            .to_string_lossy()
            .replace('\\', "/");
        if path == "manifest.json" {
            let mut s = String::new();
            entry
                .read_to_string(&mut s)
                .map_err(|e| format!("manifest 읽기 실패: {e}"))?;
            return serde_json::from_str(&s).map_err(|e| format!("manifest 파싱 실패: {e}"));
        }
    }
    Err("manifest.json을 찾을 수 없습니다(올바른 .appbak 파일이 아님).".to_owned())
}

/// 아카이브를 이 PC에 복원한다.
pub fn restore_backup(req: &RestoreRequest) -> RestoreResult {
    let manifest = match inspect_backup(&req.archive_path) {
        Ok(m) => m,
        Err(e) => {
            return RestoreResult {
                success: false,
                message: e,
                restored_entries: 0,
            }
        }
    };

    let file = match File::open(&req.archive_path) {
        Ok(f) => f,
        Err(e) => {
            return RestoreResult {
                success: false,
                message: format!("열기 실패: {e}"),
                restored_entries: 0,
            }
        }
    };
    let decoder = match zstd::Decoder::new(file) {
        Ok(d) => d,
        Err(e) => {
            return RestoreResult {
                success: false,
                message: format!("압축 해제 실패: {e}"),
                restored_entries: 0,
            }
        }
    };
    let mut archive = tar::Archive::new(decoder);

    let mut restored_files = 0usize;
    let mut restored_reg = 0usize;
    let mut failed_reg = 0usize;
    let mut touched = std::collections::HashSet::new();
    let entries_iter = match archive.entries() {
        Ok(it) => it,
        Err(e) => {
            return RestoreResult {
                success: false,
                message: format!("아카이브 읽기 실패: {e}"),
                restored_entries: 0,
            }
        }
    };

    for entry in entries_iter {
        let mut entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let in_path = match entry.path() {
            Ok(p) => p.to_string_lossy().replace('\\', "/"),
            Err(_) => continue,
        };
        if in_path == "manifest.json" || in_path.is_empty() {
            continue;
        }
        // 레지스트리 노드(reg/{i}.json)면 해당 키로 복원.
        if let Some(re) = manifest.registry.iter().find(|r| r.archive_name == in_path) {
            let mut s = String::new();
            if entry.read_to_string(&mut s).is_ok() {
                if let Ok(node) = serde_json::from_str::<RegNode>(&s) {
                    // HKLM 등은 일반 권한으로 복원 실패할 수 있음 → 실패 카운트.
                    if import_key(&re.key_path, &node).is_ok() {
                        restored_reg += 1;
                    } else {
                        failed_reg += 1;
                    }
                }
            }
            continue;
        }
        // "e{idx}/<나머지>" 파싱
        let Some((prefix, rest)) = in_path.split_once('/') else {
            continue;
        };
        let Some(idx) = prefix.strip_prefix('e').and_then(|s| s.parse::<usize>().ok()) else {
            continue;
        };
        let Some(be) = manifest.entries.get(idx) else {
            continue;
        };
        let Some(root) = resolve_root(&be.root_kind) else {
            continue;
        };

        // 대상 = root / relative_path / rest
        let mut target = PathBuf::from(root);
        if !be.relative_path.is_empty() {
            target.push(&be.relative_path);
        }
        target.push(rest.replace('/', "\\"));

        // 덮어쓰기 옵션.
        if target.exists() && !req.overwrite {
            continue;
        }
        if let Some(parent) = target.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if entry.unpack(&target).is_ok() {
            touched.insert(idx);
            restored_files += 1;
        }
    }

    let restored_total = touched.len() + restored_reg;
    let mut message = if restored_files > 0 || restored_reg > 0 {
        format!(
            "'{}' 백업에서 폴더 {}개({}개 파일), 레지스트리 {}개를 복원했습니다.",
            manifest.name,
            touched.len(),
            restored_files,
            restored_reg
        )
    } else {
        "복원된 항목이 없습니다(이미 존재하거나 대상 경로 없음).".to_owned()
    };
    if failed_reg > 0 {
        message.push_str(&format!(
            "\n레지스트리 {failed_reg}개는 복원하지 못했습니다(HKLM 등은 관리자 권한이 필요)."
        ));
    }

    RestoreResult {
        success: restored_files > 0 || restored_reg > 0,
        message,
        restored_entries: restored_total,
    }
}

//! core_engine — 앱 클리너의 Rust 코어 엔진.
//!
//! C ABI(`extern "C"`)로 함수를 노출하고, C# UI에서 P/Invoke로 호출한다.
//! 문자열은 UTF-8 + NUL 종단(C 문자열)으로 주고받으며,
//! Rust가 할당한 문자열은 반드시 `free_string`으로 되돌려 해제해야 한다.

use std::ffi::{CStr, CString};
use std::os::raw::c_char;

pub mod backup;
pub mod recycle;
pub mod remover;
pub mod residue;
pub mod scanner;
pub mod shortcut;

/// Rust 쪽에서 만든 C 문자열 포인터를 C#에 넘겨주기 위한 헬퍼.
fn to_c_string(s: String) -> *mut c_char {
    // CString::new 는 내부에 NUL이 없으면 성공. 만약 있으면 빈 문자열로 대체.
    match CString::new(s) {
        Ok(c) => c.into_raw(),
        Err(_) => CString::new("").unwrap().into_raw(),
    }
}

/// 입력 C 문자열 포인터를 안전하게 소유 String으로 변환(null이면 빈 문자열).
///
/// # Safety
/// `p`는 유효한 NUL 종단 UTF-8 C 문자열이거나 null이어야 한다.
unsafe fn ptr_to_string(p: *const c_char) -> String {
    if p.is_null() {
        String::new()
    } else {
        CStr::from_ptr(p).to_string_lossy().into_owned()
    }
}

/// catch_unwind 의 패닉 페이로드에서 메시지를 뽑아낸다.
fn panic_msg(e: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = e.downcast_ref::<&str>() {
        (*s).to_owned()
    } else if let Some(s) = e.downcast_ref::<String>() {
        s.clone()
    } else {
        "알 수 없는 내부 오류".to_owned()
    }
}

/// FFI 본문(JSON 문자열 반환)을 패닉으로부터 보호한다.
/// 패닉이 나면 abort 대신 `on_panic(메시지)`로 만든 JSON을 돌려준다.
fn guard_json<F, P>(body: F, on_panic: P) -> *mut c_char
where
    F: FnOnce() -> String + std::panic::UnwindSafe,
    P: FnOnce(String) -> String,
{
    let out = std::panic::catch_unwind(body).unwrap_or_else(|e| on_panic(panic_msg(&e)));
    to_c_string(out)
}

/// FFI 왕복 검증용 핑.
///
/// C#에서 넘긴 이름(UTF-8 C 문자열)을 받아 인사 문자열을 만들어 돌려준다.
/// 반환된 포인터는 호출 측이 `free_string`으로 해제해야 한다.
///
/// # Safety
/// `name`은 유효한 NUL 종단 UTF-8 C 문자열을 가리켜야 한다(또는 null).
#[no_mangle]
pub unsafe extern "C" fn ping(name: *const c_char) -> *mut c_char {
    let who = if name.is_null() {
        "anonymous".to_owned()
    } else {
        // 넘어온 C 문자열을 안전하게 Rust 문자열로 변환.
        CStr::from_ptr(name).to_string_lossy().into_owned()
    };

    let version = env!("CARGO_PKG_VERSION");
    to_c_string(format!(
        "core_engine v{version} (Rust) -> C# 연결 성공! 안녕하세요, {who}님."
    ))
}

/// 설치된 앱 목록을 JSON 배열 문자열로 반환한다.
///
/// 반환 형식: `[{"id":..,"name":..,"publisher":..,"size_mb":..,...}, ...]`
/// 반환된 포인터는 호출 측이 `free_string`으로 해제해야 한다.
#[no_mangle]
pub extern "C" fn scan_installed_apps() -> *mut c_char {
    let apps = scanner::scan_installed_apps();
    let json = serde_json::to_string(&apps).unwrap_or_else(|_| "[]".to_owned());
    to_c_string(json)
}

/// 정식 언인스톨러를 실행해 앱을 제거한다. (Phase 2)
///
/// `request_json`: `{"uninstall_string": "...", "name": "..."}`
/// 반환: `{"success": bool, "message": "...", "exit_code": int|null}`
/// 반환된 포인터는 호출 측이 `free_string`으로 해제해야 한다.
///
/// # Safety
/// `request_json`은 유효한 NUL 종단 UTF-8 C 문자열이어야 한다.
#[no_mangle]
pub unsafe extern "C" fn uninstall_app(request_json: *const c_char) -> *mut c_char {
    let json = ptr_to_string(request_json);
    guard_json(
        move || {
            let result = match serde_json::from_str::<remover::UninstallRequest>(&json) {
                Ok(req) => remover::uninstall(&req),
                Err(e) => remover::UninstallResult {
                    success: false,
                    message: format!("요청 파싱 실패: {e}"),
                    exit_code: None,
                },
            };
            serde_json::to_string(&result).unwrap_or_else(|_| {
                "{\"success\":false,\"message\":\"결과 직렬화 실패\",\"exit_code\":null}".to_owned()
            })
        },
        |m| {
            serde_json::to_string(&remover::UninstallResult {
                success: false,
                message: format!("내부 오류(panic): {m}"),
                exit_code: None,
            })
            .unwrap_or_else(|_| "{\"success\":false,\"message\":\"내부 오류\",\"exit_code\":null}".to_owned())
        },
    )
}

/// 정식 제거 후 남은 잔여물(파일/폴더/레지스트리 키) 후보를 찾는다. (Phase 3)
///
/// `request_json`: `{"name":"..","publisher":"..","install_location":".."}`
/// 반환: `[{"path":"..","kind":"folder|registry","size_mb":N}, ...]`
/// ⚠️ 자동 삭제하지 않는다. 사용자가 확인 후 delete_residue로 지운다.
///
/// # Safety
/// `request_json`은 유효한 NUL 종단 UTF-8 C 문자열이어야 한다.
#[no_mangle]
pub unsafe extern "C" fn find_residue(request_json: *const c_char) -> *mut c_char {
    let json = if request_json.is_null() {
        String::new()
    } else {
        CStr::from_ptr(request_json).to_string_lossy().into_owned()
    };
    let items = match serde_json::from_str::<residue::FindRequest>(&json) {
        Ok(req) => residue::find_residue(&req),
        Err(_) => Vec::new(),
    };
    to_c_string(serde_json::to_string(&items).unwrap_or_else(|_| "[]".to_owned()))
}

/// 사용자가 선택한 잔여물만 삭제한다(폴더→휴지통, 레지스트리→삭제). (Phase 3)
///
/// `request_json`: `{"items":[{"path":..,"kind":..,"size_mb":..}, ...]}`
/// 반환: `{"deleted":[..],"failed":[{"path":..,"error":..}],"recycled_paths":[..]}`
///
/// # Safety
/// `request_json`은 유효한 NUL 종단 UTF-8 C 문자열이어야 한다.
#[no_mangle]
pub unsafe extern "C" fn delete_residue(request_json: *const c_char) -> *mut c_char {
    let json = ptr_to_string(request_json);
    guard_json(
        move || {
            let result = match serde_json::from_str::<residue::DeleteRequest>(&json) {
                Ok(req) => residue::delete_residue(&req),
                Err(e) => residue::DeleteResult {
                    deleted: Vec::new(),
                    failed: vec![residue::FailedItem {
                        path: String::new(),
                        error: format!("요청 파싱 실패: {e}"),
                    }],
                    recycled_paths: Vec::new(),
                },
            };
            serde_json::to_string(&result)
                .unwrap_or_else(|_| "{\"deleted\":[],\"failed\":[],\"recycled_paths\":[]}".to_owned())
        },
        |m| {
            serde_json::to_string(&residue::DeleteResult {
                deleted: Vec::new(),
                failed: vec![residue::FailedItem {
                    path: String::new(),
                    error: format!("내부 오류(panic): {m}"),
                }],
                recycled_paths: Vec::new(),
            })
            .unwrap_or_else(|_| "{\"deleted\":[],\"failed\":[],\"recycled_paths\":[]}".to_owned())
        },
    )
}

/// 휴지통에서 추적된 항목만 영구삭제한다. (F6)
///
/// `request_json`: `{"paths":["C:\\...\\Foo", ...]}` (잔여물 삭제 시 받은 recycled_paths)
/// 반환: `{"purged":N,"message":".."}`
///
/// # Safety
/// `request_json`은 유효한 NUL 종단 UTF-8 C 문자열이어야 한다.
#[no_mangle]
pub unsafe extern "C" fn permanently_delete(request_json: *const c_char) -> *mut c_char {
    let json = ptr_to_string(request_json);
    let panic_json = |m: String| {
        serde_json::to_string(&recycle::PurgeResult {
            purged: 0,
            message: format!("내부 오류(panic): {m}"),
        })
        .unwrap_or_else(|_| "{\"purged\":0,\"message\":\"내부 오류\"}".to_owned())
    };
    guard_json(
        move || {
            let result = match serde_json::from_str::<recycle::PurgeRequest>(&json) {
                Ok(req) => recycle::permanently_delete(&req),
                Err(e) => recycle::PurgeResult {
                    purged: 0,
                    message: format!("요청 파싱 실패: {e}"),
                },
            };
            serde_json::to_string(&result)
                .unwrap_or_else(|_| "{\"purged\":0,\"message\":\"직렬화 실패\"}".to_owned())
        },
        panic_json,
    )
}

/// 휴지통 전체를 비운다. (⚠️ 다른 모든 휴지통 파일도 영구삭제됨) (F6)
///
/// 반환: `{"purged":N,"message":".."}`
#[no_mangle]
pub extern "C" fn empty_recycle_bin() -> *mut c_char {
    guard_json(
        || {
            let result = recycle::empty_recycle_bin();
            serde_json::to_string(&result)
                .unwrap_or_else(|_| "{\"purged\":0,\"message\":\"직렬화 실패\"}".to_owned())
        },
        |m| format!("{{\"purged\":0,\"message\":\"내부 오류(panic): {m}\"}}"),
    )
}

/// 바로가기 대상 exe 경로로 설치 앱을 찾는다. (F3, 드래그앤드롭)
///
/// `target_exe`: .lnk가 가리키는 실행파일 경로
/// 반환: 매칭된 앱의 AppInfo JSON, 없으면 `null`
///
/// # Safety
/// `target_exe`는 유효한 NUL 종단 UTF-8 C 문자열이어야 한다.
#[no_mangle]
pub unsafe extern "C" fn match_shortcut(target_exe: *const c_char) -> *mut c_char {
    let target = if target_exe.is_null() {
        String::new()
    } else {
        CStr::from_ptr(target_exe).to_string_lossy().into_owned()
    };
    let json = match shortcut::match_shortcut(&target) {
        Some(app) => serde_json::to_string(&app).unwrap_or_else(|_| "null".to_owned()),
        None => "null".to_owned(),
    };
    to_c_string(json)
}

/// 앱 데이터를 .appbak(tar+zstd)으로 백업한다. (Phase 7)
///
/// `request_json`: `{"name":..,"publisher":..,"version":..,"paths":[..],"output_path":..}`
/// 반환: `{"success":bool,"message":..,"entry_count":N}`
///
/// # Safety
/// `request_json`은 유효한 NUL 종단 UTF-8 C 문자열이어야 한다.
#[no_mangle]
pub unsafe extern "C" fn create_backup(request_json: *const c_char) -> *mut c_char {
    let json = ptr_to_string(request_json);
    guard_json(
        move || {
            let result = match serde_json::from_str::<backup::CreateRequest>(&json) {
                Ok(req) => backup::create_backup(&req),
                Err(e) => backup::CreateResult {
                    success: false,
                    message: format!("요청 파싱 실패: {e}"),
                    entry_count: 0,
                },
            };
            serde_json::to_string(&result)
                .unwrap_or_else(|_| "{\"success\":false,\"message\":\"직렬화 실패\",\"entry_count\":0}".to_owned())
        },
        |m| format!("{{\"success\":false,\"message\":\"내부 오류(panic): {m}\",\"entry_count\":0}}"),
    )
}

/// .appbak에서 manifest만 읽어 미리보기용으로 반환한다. (Phase 7)
///
/// `archive_path`: .appbak 경로. 반환: Manifest JSON 또는 `{"error":".."}`
///
/// # Safety
/// `archive_path`는 유효한 NUL 종단 UTF-8 C 문자열이어야 한다.
#[no_mangle]
pub unsafe extern "C" fn inspect_backup(archive_path: *const c_char) -> *mut c_char {
    let path = ptr_to_string(archive_path);
    guard_json(
        move || match backup::inspect_backup(&path) {
            Ok(m) => serde_json::to_string(&m).unwrap_or_else(|_| "{\"error\":\"직렬화 실패\"}".to_owned()),
            Err(e) => format!("{{\"error\":{}}}", serde_json::to_string(&e).unwrap_or_else(|_| "\"오류\"".to_owned())),
        },
        |m| format!("{{\"error\":\"내부 오류(panic): {m}\"}}"),
    )
}

/// .appbak을 이 PC에 복원한다. (Phase 7)
///
/// `request_json`: `{"archive_path":..,"overwrite":bool}`
/// 반환: `{"success":bool,"message":..,"restored_entries":N}`
///
/// # Safety
/// `request_json`은 유효한 NUL 종단 UTF-8 C 문자열이어야 한다.
#[no_mangle]
pub unsafe extern "C" fn restore_backup(request_json: *const c_char) -> *mut c_char {
    let json = ptr_to_string(request_json);
    guard_json(
        move || {
            let result = match serde_json::from_str::<backup::RestoreRequest>(&json) {
                Ok(req) => backup::restore_backup(&req),
                Err(e) => backup::RestoreResult {
                    success: false,
                    message: format!("요청 파싱 실패: {e}"),
                    restored_entries: 0,
                },
            };
            serde_json::to_string(&result)
                .unwrap_or_else(|_| "{\"success\":false,\"message\":\"직렬화 실패\",\"restored_entries\":0}".to_owned())
        },
        |m| format!("{{\"success\":false,\"message\":\"내부 오류(panic): {m}\",\"restored_entries\":0}}"),
    )
}

/// `ping` 등이 반환한 문자열의 메모리를 해제한다.
///
/// # Safety
/// `ptr`은 이 라이브러리가 반환한 포인터이거나 null이어야 하며, 두 번 해제하면 안 된다.
#[no_mangle]
pub unsafe extern "C" fn free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        // into_raw 로 넘겼던 소유권을 다시 가져와 drop 시킨다.
        drop(CString::from_raw(ptr));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ping_returns_greeting() {
        let name = CString::new("beomjin").unwrap();
        unsafe {
            let ptr = ping(name.as_ptr());
            let result = CStr::from_ptr(ptr).to_string_lossy().into_owned();
            assert!(result.contains("beomjin"));
            assert!(result.contains("core_engine"));
            free_string(ptr);
        }
    }

    #[test]
    fn ping_handles_null() {
        unsafe {
            let ptr = ping(std::ptr::null());
            let result = CStr::from_ptr(ptr).to_string_lossy().into_owned();
            assert!(result.contains("anonymous"));
            free_string(ptr);
        }
    }
}

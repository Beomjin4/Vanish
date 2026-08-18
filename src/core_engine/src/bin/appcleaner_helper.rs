//! 관리자 권한이 필요한 잔여물 삭제를 수행하는 헬퍼.
//!
//! 메인 앱(일반권한)이 ShellExecute("runas")로 이 exe를 띄워 "삭제만" 승격 실행한다.
//! (삭제때만 승격 정책). 콘솔 창이 뜨지 않도록 windows 서브시스템으로 빌드한다.
//!
//! 사용법: appcleaner_helper.exe <input.json> <output.json>
//!   input.json  = DeleteRequest JSON  ({"items":[{path,kind,size_mb}, ...]})
//!   output.json = DeleteResult JSON 기록

#![windows_subsystem = "windows"]

use std::process::exit;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        exit(2);
    }

    let input = match std::fs::read_to_string(&args[1]) {
        Ok(s) => s,
        Err(_) => exit(3),
    };
    // UTF-8 BOM이 있으면 제거(일부 기록기가 BOM을 붙임).
    let input = input.strip_prefix('\u{feff}').unwrap_or(&input);

    let result = match serde_json::from_str::<core_engine::residue::DeleteRequest>(&input) {
        Ok(req) => core_engine::residue::delete_residue(&req),
        Err(e) => core_engine::residue::DeleteResult {
            deleted: Vec::new(),
            failed: vec![core_engine::residue::FailedItem {
                path: String::new(),
                error: format!("요청 파싱 실패: {e}"),
            }],
            recycled_paths: Vec::new(),
        },
    };

    let out = serde_json::to_string(&result).unwrap_or_else(|_| {
        "{\"deleted\":[],\"failed\":[],\"recycled_paths\":[]}".to_owned()
    });
    let _ = std::fs::write(&args[2], out);
}

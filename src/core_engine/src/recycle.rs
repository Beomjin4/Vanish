//! 휴지통 영구삭제 (F6).
//!
//! 잔여물 삭제(Phase 3)는 폴더를 휴지통으로 보낸다. 여기서는:
//!   1. 방금 보낸 항목만 골라 영구삭제 (다른 휴지통 파일은 보존)
//!   2. 휴지통 전체 비우기 (경고용)
//!
//! Windows에는 "휴지통 내 특정 항목만 비우기" 표준 API가 없으므로,
//! `trash::os_limited`로 휴지통 목록을 받아 원본 경로가 일치하는 것만 purge 한다.

use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct PurgeRequest {
    /// 영구삭제할 원본 경로들(잔여물 삭제 시 추적한 recycled_paths).
    pub paths: Vec<String>,
}

#[derive(Serialize)]
pub struct PurgeResult {
    pub purged: usize,
    pub message: String,
}

/// 원본 경로가 `paths`에 포함된 휴지통 항목만 영구삭제한다.
pub fn permanently_delete(req: &PurgeRequest) -> PurgeResult {
    // 비교를 위해 대상 경로를 소문자로 정규화.
    let targets: Vec<String> = req.paths.iter().map(|p| p.to_lowercase()).collect();

    let all = match trash::os_limited::list() {
        Ok(items) => items,
        Err(e) => {
            return PurgeResult {
                purged: 0,
                message: format!("휴지통 목록 조회 실패: {e}"),
            }
        }
    };

    let matched: Vec<_> = all
        .into_iter()
        .filter(|item| {
            // 항목의 원본 전체 경로.
            let original_l = item.original_path().to_string_lossy().to_lowercase();
            targets.iter().any(|t| *t == original_l)
        })
        .collect();

    let count = matched.len();
    match trash::os_limited::purge_all(matched) {
        Ok(_) => PurgeResult {
            purged: count,
            message: format!("{count}개 항목을 영구삭제했습니다."),
        },
        Err(e) => PurgeResult {
            purged: 0,
            message: format!("영구삭제 실패: {e}"),
        },
    }
}

/// 휴지통 전체를 비운다. (⚠️ 다른 모든 휴지통 파일도 사라짐)
pub fn empty_recycle_bin() -> PurgeResult {
    let all = match trash::os_limited::list() {
        Ok(items) => items,
        Err(e) => {
            return PurgeResult {
                purged: 0,
                message: format!("휴지통 목록 조회 실패: {e}"),
            }
        }
    };
    let count = all.len();
    match trash::os_limited::purge_all(all) {
        Ok(_) => PurgeResult {
            purged: count,
            message: format!("휴지통을 비웠습니다 ({count}개 항목 영구삭제)."),
        },
        Err(e) => PurgeResult {
            purged: 0,
            message: format!("휴지통 비우기 실패: {e}"),
        },
    }
}

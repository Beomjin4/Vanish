namespace AppCleaner_UI.Models;

/// <summary>휴지통 영구삭제 결과. Rust permanently_delete()/empty_recycle_bin()과 매핑.</summary>
public sealed class PurgeResult
{
    public int Purged { get; set; }
    public string Message { get; set; } = "";
}

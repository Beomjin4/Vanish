using System.Collections.Generic;

namespace AppCleaner_UI.Models;

/// <summary>잔여물 삭제 결과. Rust delete_residue()의 JSON과 매핑된다.</summary>
public sealed class DeleteResult
{
    public List<string> Deleted { get; set; } = new();
    public List<FailedItem> Failed { get; set; } = new();

    /// <summary>휴지통으로 보낸 경로(이후 영구삭제(F6)용 추적).</summary>
    public List<string> RecycledPaths { get; set; } = new();
}

public sealed class FailedItem
{
    public string Path { get; set; } = "";
    public string Error { get; set; } = "";
}

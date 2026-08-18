namespace AppCleaner_UI.Models;

/// <summary>제거 작업 결과. Rust uninstall_app()의 JSON 응답과 매핑된다.</summary>
public sealed class UninstallResult
{
    public bool Success { get; set; }
    public string Message { get; set; } = "";
    public int? ExitCode { get; set; }
}

using System.Text.Json.Serialization;
using CommunityToolkit.Mvvm.ComponentModel;

namespace AppCleaner_UI.Models;

/// <summary>
/// 잔여물 후보 1건. Rust find_residue()의 JSON과 매핑되며,
/// UI 체크박스 선택 상태(IsSelected)를 함께 들고 있다.
/// </summary>
public partial class ResidueItem : ObservableObject
{
    public string Path { get; set; } = "";

    /// <summary>"folder" | "registry"</summary>
    public string Kind { get; set; } = "";

    public ulong SizeMb { get; set; }

    /// <summary>체크박스 선택 상태(기본 해제 — 안전). 직렬화 제외.</summary>
    [JsonIgnore]
    [ObservableProperty]
    public partial bool IsSelected { get; set; }

    /// <summary>표시용 종류 라벨.</summary>
    [JsonIgnore]
    public string KindLabel => Kind == "registry" ? "레지스트리" : "폴더";

    /// <summary>표시용 용량.</summary>
    [JsonIgnore]
    public string SizeDisplay => Kind == "registry" ? "" : (SizeMb > 0 ? $"{SizeMb:N0} MB" : "<1 MB");
}

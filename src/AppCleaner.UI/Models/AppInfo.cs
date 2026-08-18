using System;
using System.Text.Json.Serialization;
using System.Threading.Tasks;
using CommunityToolkit.Mvvm.ComponentModel;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Imaging;
using Windows.Storage;
using Windows.Storage.FileProperties;

namespace AppCleaner_UI.Models;

/// <summary>
/// 설치된 앱 1건. Rust 코어 엔진이 반환하는 JSON과 매핑되며,
/// 아이콘 이미지(IconSource)는 UI에서 비동기로 로드한다.
/// </summary>
public partial class AppInfo : ObservableObject
{
    public string Id { get; set; } = "";
    public string Name { get; set; } = "";
    public string Publisher { get; set; } = "";
    public string Version { get; set; } = "";
    public string InstallLocation { get; set; } = "";
    public string UninstallString { get; set; } = "";
    public string Icon { get; set; } = "";
    public ulong SizeMb { get; set; }
    public string InstallDate { get; set; } = "";
    public string Source { get; set; } = "";

    // ===== MSIX/APPX(패키지 앱)용 — Win32 앱에서는 기본값 =====
    /// <summary>Store/MSIX 패키지 앱인가.</summary>
    [JsonIgnore]
    public bool IsPackaged { get; set; }

    /// <summary>패키지 전체 이름(제거 시 사용).</summary>
    [JsonIgnore]
    public string PackageFullName { get; set; } = "";

    /// <summary>패키지 패밀리 이름(데이터 폴더 %LocalAppData%\Packages\&lt;PFN&gt;).</summary>
    [JsonIgnore]
    public string PackageFamilyName { get; set; } = "";

    /// <summary>패키지 로고 URI(아이콘용).</summary>
    [JsonIgnore]
    public Uri? LogoUri { get; set; }

    /// <summary>아이콘 이미지(비동기 로드, 실패 시 null). 직렬화 제외.</summary>
    [JsonIgnore]
    [ObservableProperty]
    public partial ImageSource? IconSource { get; set; }

    [JsonIgnore]
    private bool _iconRequested;

    /// <summary>화면 표시용 용량 문자열 (0이면 "-").</summary>
    [JsonIgnore]
    public string SizeDisplay => SizeMb > 0 ? $"{SizeMb:N0} MB" : "-";

    /// <summary>제조사가 비어 있으면 "(알 수 없음)".</summary>
    [JsonIgnore]
    public string PublisherDisplay => string.IsNullOrWhiteSpace(Publisher) ? "(알 수 없음)" : Publisher;

    /// <summary>설치 위치 표시용(비어 있으면 안내 문구).</summary>
    [JsonIgnore]
    public string InstallLocationDisplay =>
        string.IsNullOrWhiteSpace(InstallLocation) ? "(설치 위치 정보 없음)" : InstallLocation;

    /// <summary>
    /// 아이콘을 비동기로 로드한다(최초 1회). DisplayIcon 경로나 설치 폴더의 exe에서 추출.
    /// UI 스레드에서 호출할 것(BitmapImage 생성 때문).
    /// </summary>
    public async Task EnsureIconAsync()
    {
        if (_iconRequested)
        {
            return;
        }
        _iconRequested = true;

        // 패키지 앱은 로고 URI로 직접 로드.
        if (IsPackaged)
        {
            if (LogoUri is not null)
            {
                try { IconSource = new BitmapImage(LogoUri); }
                catch { /* 무시 */ }
            }
            return;
        }

        string? path = ResolveIconFilePath();
        if (path is null)
        {
            return;
        }

        try
        {
            StorageFile file = await StorageFile.GetFileFromPathAsync(path);
            using StorageItemThumbnail thumb =
                await file.GetThumbnailAsync(ThumbnailMode.SingleItem, 64);
            if (thumb is not null && thumb.Type == ThumbnailType.Image)
            {
                var bmp = new BitmapImage();
                await bmp.SetSourceAsync(thumb);
                IconSource = bmp;
            }
        }
        catch
        {
            // 아이콘 로드 실패는 무시(기본 자리표시자 표시).
        }
    }

    /// <summary>아이콘을 뽑아낼 파일 경로(.ico/.exe). 없으면 null.</summary>
    private string? ResolveIconFilePath()
    {
        // DisplayIcon: "path,index" / 따옴표 형태 정리.
        string raw = Icon.Trim();
        if (!string.IsNullOrEmpty(raw))
        {
            string p = raw.StartsWith('"')
                ? raw[1..].Split('"', 2)[0]
                : raw.Split(',')[0];
            p = p.Trim();
            if (p.Length > 0 && System.IO.File.Exists(p))
            {
                return p;
            }
        }
        return null;
    }
}

using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Microsoft.UI.Dispatching;
using AppCleaner_UI.Interop;
using AppCleaner_UI.Models;

namespace AppCleaner_UI.ViewModels;

/// <summary>
/// 메인 화면 ViewModel — 설치 앱 목록 로드, 선택 앱 상세/관련 파일 표시.
/// </summary>
public partial class MainPageViewModel : ObservableObject
{
    private readonly DispatcherQueue _dispatcher = DispatcherQueue.GetForCurrentThread();
    private CancellationTokenSource? _residueCts;
    private readonly List<AppInfo> _allApps = new();

    public MainPageViewModel()
    {
        _ = LoadAppsAsync();
    }

    /// <summary>화면에 보이는 앱 목록(검색 필터 적용됨).</summary>
    public ObservableCollection<AppInfo> Apps { get; } = new();

    /// <summary>검색어(이름/제조사 부분일치).</summary>
    [ObservableProperty]
    public partial string SearchText { get; set; } = "";

    partial void OnSearchTextChanged(string value) => ApplyFilter();

    /// <summary>검색어로 _allApps를 걸러 Apps에 반영.</summary>
    private void ApplyFilter()
    {
        string q = SearchText?.Trim() ?? "";
        IEnumerable<AppInfo> source = _allApps;
        if (q.Length > 0)
        {
            source = _allApps.Where(a =>
                a.Name.Contains(q, StringComparison.OrdinalIgnoreCase) ||
                a.Publisher.Contains(q, StringComparison.OrdinalIgnoreCase));
        }

        Apps.Clear();
        foreach (var app in source)
        {
            Apps.Add(app);
        }
    }

    /// <summary>선택 앱의 관련 파일(잔여물 미리보기, 읽기 전용).</summary>
    public ObservableCollection<ResidueItem> SelectedAppResidue { get; } = new();

    /// <summary>현재 선택된 앱(없으면 null).</summary>
    [ObservableProperty]
    public partial AppInfo? SelectedApp { get; set; }

    /// <summary>삭제 버튼 활성화 조건: 선택된 앱이 있고 로딩 중이 아닐 때.</summary>
    public bool CanUninstall => SelectedApp is not null && !IsLoading;

    /// <summary>관련 파일 로딩 중 여부.</summary>
    [ObservableProperty]
    public partial bool IsResidueLoading { get; set; }

    /// <summary>관련 파일 상태 문구.</summary>
    [ObservableProperty]
    public partial string ResidueStatus { get; set; } = "";

    partial void OnIsLoadingChanged(bool value) => OnPropertyChanged(nameof(CanUninstall));

    partial void OnSelectedAppChanged(AppInfo? value)
    {
        OnPropertyChanged(nameof(CanUninstall));
        _ = LoadSelectedResidueAsync(value);
    }

    /// <summary>상태 표시 텍스트(개수/로딩/오류).</summary>
    [ObservableProperty]
    public partial string Status { get; set; } = "준비됨";

    /// <summary>로딩 중 여부(프로그레스 링/버튼 비활성화용).</summary>
    [ObservableProperty]
    public partial bool IsLoading { get; set; }

    /// <summary>
    /// Rust 코어 엔진으로 설치 앱을 스캔해 목록을 채운다.
    /// </summary>
    [RelayCommand]
    private async Task LoadAppsAsync()
    {
        IsLoading = true;
        Status = "설치된 앱을 스캔하는 중...";
        try
        {
            // Win32(레지스트리) 앱 + MSIX/APPX(패키지) 앱을 함께 수집.
            var win32 = await Task.Run(CoreEngine.ScanInstalledApps);
            var packaged = await Task.Run(PackageScanner.Scan);

            _allApps.Clear();
            _allApps.AddRange(win32);
            _allApps.AddRange(packaged);
            _allApps.Sort((a, b) => string.Compare(a.Name, b.Name, StringComparison.OrdinalIgnoreCase));
            ApplyFilter();

            Status = $"설치된 앱 {_allApps.Count}개 (일반 {win32.Count} · 패키지 {packaged.Count})";

            // 아이콘은 목록 표시 후 비동기로 채운다(UI 블로킹 방지).
            _ = LoadIconsAsync();
        }
        catch (Exception ex)
        {
            Status = $"스캔 실패: {ex.Message}";
        }
        finally
        {
            IsLoading = false;
        }
    }

    /// <summary>각 앱 아이콘을 순차 로드(UI 스레드).</summary>
    private async Task LoadIconsAsync()
    {
        foreach (var app in _allApps.ToList())
        {
            await app.EnsureIconAsync();
        }
    }

    /// <summary>선택된 앱의 잔여물(관련 파일)을 백그라운드로 찾아 표시.</summary>
    private async Task LoadSelectedResidueAsync(AppInfo? app)
    {
        // 이전 요청 취소.
        _residueCts?.Cancel();
        var cts = new CancellationTokenSource();
        _residueCts = cts;

        SelectedAppResidue.Clear();
        if (app is null)
        {
            ResidueStatus = "";
            IsResidueLoading = false;
            return;
        }

        IsResidueLoading = true;
        ResidueStatus = "관련 파일 검색 중...";

        try
        {
            // 패키지 앱은 %LocalAppData%\Packages\<PFN> 데이터 폴더가 관련 파일.
            var items = app.IsPackaged
                ? await Task.Run(() => FindPackageResidue(app), cts.Token)
                : await Task.Run(() => CoreEngine.FindResidue(app), cts.Token);
            if (cts.Token.IsCancellationRequested)
            {
                return;
            }

            // 선택이 그대로일 때만 반영.
            if (!ReferenceEquals(_residueCts, cts))
            {
                return;
            }

            foreach (var item in items)
            {
                item.IsSelected = true; // 기본 전체 선택
                SelectedAppResidue.Add(item);
            }
            ResidueStatus = items.Count > 0
                ? $"관련 항목 {items.Count}개"
                : "발견된 관련 파일 없음";
        }
        catch (OperationCanceledException)
        {
            // 무시.
        }
        catch (Exception ex)
        {
            ResidueStatus = $"검색 실패: {ex.Message}";
        }
        finally
        {
            if (ReferenceEquals(_residueCts, cts))
            {
                IsResidueLoading = false;
            }
        }
    }

    /// <summary>패키지 앱의 데이터 폴더(%LocalAppData%\Packages\&lt;PFN&gt;)를 관련 파일로 반환.</summary>
    private static System.Collections.Generic.List<ResidueItem> FindPackageResidue(AppInfo app)
    {
        var list = new System.Collections.Generic.List<ResidueItem>();
        if (string.IsNullOrEmpty(app.PackageFamilyName))
        {
            return list;
        }
        string local = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
        string dir = System.IO.Path.Combine(local, "Packages", app.PackageFamilyName);
        if (System.IO.Directory.Exists(dir))
        {
            list.Add(new ResidueItem
            {
                Path = dir,
                Kind = "folder",
                SizeMb = DirSizeMb(dir),
            });
        }
        return list;
    }

    /// <summary>폴더 용량(MB) 대략 계산(엔트리 상한으로 과도한 스캔 방지).</summary>
    private static ulong DirSizeMb(string path)
    {
        ulong bytes = 0;
        int budget = 20000;
        try
        {
            foreach (var f in System.IO.Directory.EnumerateFiles(path, "*", System.IO.SearchOption.AllDirectories))
            {
                if (budget-- <= 0) break;
                try { bytes += (ulong)new System.IO.FileInfo(f).Length; } catch { }
            }
        }
        catch { }
        return bytes / (1024 * 1024);
    }

    /// <summary>
    /// 제거를 '실행'한다(확인은 View에서). 패키지 앱은 RemoveAsync가 실제 제거이고,
    /// Win32는 언인스톨러를 띄우기만 한다(종료코드로 판단하지 않음).
    /// 실제 제거 여부는 View가 <see cref="RefreshAndCheckRemovedAsync"/>로 재스캔해 확인.
    /// 목록 새로고침은 여기서 하지 않는다.
    /// </summary>
    public async Task<UninstallResult> UninstallAsync(AppInfo app)
    {
        IsLoading = true;
        Status = $"'{app.Name}' 제거 중...";
        try
        {
            if (app.IsPackaged)
            {
                // MSIX/APPX: PackageManager로 제거(결과가 곧 실제 제거 여부).
                return await PackageScanner.RemoveAsync(app.PackageFullName);
            }
            // Win32: ShellExecute("runas")로 언인스톨러 실행(STA 스레드).
            var result = await StaRunner.RunAsync(() => CoreEngine.UninstallApp(app));
            Status = result.Message;
            return result;
        }
        finally
        {
            IsLoading = false;
        }
    }

    /// <summary>목록을 다시 스캔한다.</summary>
    public Task RefreshAsync() => LoadAppsAsync();

    /// <summary>목록을 재스캔한 뒤, 해당 앱이 더 이상 설치돼 있지 않으면 true(제거됨).</summary>
    public async Task<bool> RefreshAndCheckRemovedAsync(AppInfo app)
    {
        await LoadAppsAsync();
        return !_allApps.Any(a => a.Id == app.Id);
    }
}

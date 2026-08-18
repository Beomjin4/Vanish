using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading.Tasks;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Windows.ApplicationModel.DataTransfer;
using Windows.Storage;
using Windows.Storage.Pickers;
using AppCleaner_UI.Interop;
using AppCleaner_UI.Models;
using AppCleaner_UI.ViewModels;
using AppCleaner_UI.Views;

// To learn more about WinUI, the WinUI project structure,
// and more about our project templates, see: http://aka.ms/winui-project-info.

namespace AppCleaner_UI;

/// <summary>
/// The main content page displayed inside the application window.
/// </summary>
public sealed partial class MainPage : Page
{
    public MainPageViewModel ViewModel { get; } = new();

    public MainPage()
    {
        InitializeComponent();

        // 앱을 새로 선택하면 관련 파일은 기본 전체 선택이므로 박스도 체크 상태로 동기화.
        ViewModel.PropertyChanged += (_, e) =>
        {
            if (e.PropertyName == nameof(ViewModel.SelectedApp))
            {
                SelectAllResidueBox.IsChecked = true;
            }
        };
    }

    /// <summary>삭제 버튼 클릭 — 선택된 앱으로 삭제 흐름 실행.</summary>
    private async void OnUninstallClick(object sender, RoutedEventArgs e)
    {
        if (ViewModel.SelectedApp is { } app)
        {
            await RunUninstallFlowAsync(app);
        }
    }

    /// <summary>
    /// 백업 버튼 — 체크한 관련 파일(폴더)을 .appbak으로 압축 저장. (Phase 7)
    /// </summary>
    private async void OnBackupClick(object sender, RoutedEventArgs e)
    {
        if (ViewModel.SelectedApp is not { } app)
        {
            return;
        }

        // 체크한 폴더 + 레지스트리 키를 백업 대상으로.
        var folders = ViewModel.SelectedAppResidue
            .Where(r => r.IsSelected && r.Kind == "folder")
            .Select(r => r.Path)
            .ToList();
        var regKeys = ViewModel.SelectedAppResidue
            .Where(r => r.IsSelected && r.Kind == "registry")
            .Select(r => r.Path)
            .ToList();

        if (folders.Count == 0 && regKeys.Count == 0)
        {
            var none = new ContentDialog
            {
                XamlRoot = XamlRoot,
                Title = "백업할 항목 없음",
                Content = "관련 파일에서 백업할 폴더나 레지스트리를 먼저 체크하세요.",
                CloseButtonText = "확인",
            };
            await none.ShowAsync();
            return;
        }

        // 저장 위치 선택.
        var picker = new FileSavePicker
        {
            SuggestedStartLocation = PickerLocationId.DocumentsLibrary,
            SuggestedFileName = SanitizeFileName(app.Name) + "_backup",
        };
        picker.FileTypeChoices.Add("앱 클리너 백업", new List<string> { ".appbak" });
        WinRT.Interop.InitializeWithWindow.Initialize(picker, App.WindowHandle);

        StorageFile? file = await picker.PickSaveFileAsync();
        if (file is null)
        {
            return;
        }

        ViewModel.Status = $"'{app.Name}' 백업 중...";
        BackupCreateResult result = await Task.Run(() => CoreEngine.CreateBackup(app, folders, regKeys, file.Path));
        ViewModel.Status = result.Message;

        var done = new ContentDialog
        {
            XamlRoot = XamlRoot,
            Title = result.Success ? "백업 완료" : "백업 실패",
            Content = result.Success
                ? $"{result.Message}\n\n{file.Path}\n\n다른 PC의 앱 클리너에 이 파일을 끌어다 놓으면 복원됩니다."
                : result.Message,
            CloseButtonText = "확인",
        };
        await done.ShowAsync();
    }

    private static string SanitizeFileName(string name)
    {
        foreach (char c in System.IO.Path.GetInvalidFileNameChars())
        {
            name = name.Replace(c, '_');
        }
        return name;
    }

    /// <summary>좌측 "관련 파일" 목록의 전체 선택/해제.</summary>
    private void OnSelectAllResidue(object sender, RoutedEventArgs e)
    {
        bool select = SelectAllResidueBox.IsChecked == true;
        foreach (var item in ViewModel.SelectedAppResidue)
        {
            item.IsSelected = select;
        }
    }

    /// <summary>
    /// 앱 삭제 전체 흐름: 확인 → 정식 제거 → (좌측에서 체크한)관련 파일 정리 →
    /// (선택)영구삭제 → 결과 알림. 버튼 클릭과 드래그앤드롭이 공유한다.
    /// </summary>
    private async Task RunUninstallFlowAsync(AppInfo app)
    {
        // 좌측에서 체크한 관련 파일을 먼저 스냅샷
        // (제거 성공 시 목록 새로고침으로 선택/잔여물 목록이 사라지기 때문).
        var checkedResidue = ViewModel.SelectedAppResidue.Where(r => r.IsSelected).ToList();

        // 1) 삭제 확인 (취소가 기본 버튼 — 실수 방지).
        string extra = checkedResidue.Count > 0
            ? $"\n\n체크한 관련 파일 {checkedResidue.Count}개도 함께 정리합니다(폴더는 휴지통)."
            : "";
        string how = app.IsPackaged
            ? "Store/패키지 앱으로 즉시 제거됩니다."
            : "해당 앱의 정식 제거 프로그램이 실행됩니다. 마법사가 뜨면 안내에 따라 진행하세요.";
        var confirm = new ContentDialog
        {
            XamlRoot = XamlRoot,
            Title = "앱 삭제",
            Content = $"'{app.Name}'을(를) 삭제하시겠습니까?\n\n" + how + extra,
            PrimaryButtonText = "삭제",
            CloseButtonText = "취소",
            DefaultButton = ContentDialogButton.Close,
        };
        if (await confirm.ShowAsync() != ContentDialogResult.Primary)
        {
            return;
        }

        // 2) 제거 실행.
        UninstallResult result = await ViewModel.UninstallAsync(app);
        if (!result.Success)
        {
            // 실행 자체가 실패(예: UAC 취소, 언인스톨러 없음).
            var fail = new ContentDialog
            {
                XamlRoot = XamlRoot,
                Title = "삭제 미완료",
                Content = result.Message,
                CloseButtonText = "확인",
            };
            await fail.ShowAsync();
            await ViewModel.RefreshAsync();
            return;
        }

        // 3) 실제 제거 여부 판정.
        //    - 패키지 앱: RemoveAsync 결과가 곧 제거 여부.
        //    - Win32: 언인스톨러(마법사)가 따로 뜰 수 있으니, 완료 후 재스캔으로 확인.
        bool removed;
        if (app.IsPackaged)
        {
            await ViewModel.RefreshAsync();
            removed = true;
        }
        else
        {
            var verify = new ContentDialog
            {
                XamlRoot = XamlRoot,
                Title = "제거 확인",
                Content = $"'{app.Name}' 제거 프로그램을 실행했습니다.\n\n" +
                          "제거 마법사가 떴다면 끝까지 진행한 뒤 아래 '제거 완료'를 누르세요.\n" +
                          "(아직 진행 중이거나 취소했다면 '취소')",
                PrimaryButtonText = "제거 완료",
                CloseButtonText = "취소",
                DefaultButton = ContentDialogButton.Primary,
            };
            if (await verify.ShowAsync() != ContentDialogResult.Primary)
            {
                ViewModel.Status = "제거 확인을 취소했습니다.";
                await ViewModel.RefreshAsync();
                return;
            }
            removed = await ViewModel.RefreshAndCheckRemovedAsync(app);
        }

        // 4) 제거됐으면 관련 파일 정리, 아니면 안내.
        if (!removed)
        {
            var still = new ContentDialog
            {
                XamlRoot = XamlRoot,
                Title = "아직 제거되지 않음",
                Content = $"'{app.Name}'이(가) 여전히 설치되어 있습니다.\n마법사를 취소했거나 제거가 완료되지 않은 것 같습니다.",
                CloseButtonText = "확인",
            };
            await still.ShowAsync();
            return;
        }

        if (checkedResidue.Count > 0)
        {
            await CleanupCheckedResidueAsync(checkedResidue);
        }

        var done = new ContentDialog
        {
            XamlRoot = XamlRoot,
            Title = "삭제 완료",
            Content = $"'{app.Name}'을(를) 제거했습니다.",
            CloseButtonText = "확인",
        };
        await done.ShowAsync();
    }

    /// <summary>체크한 관련 파일을 휴지통/삭제하고, 실패·영구삭제를 안내한다.</summary>
    private async Task CleanupCheckedResidueAsync(IReadOnlyList<ResidueItem> selected)
    {
        // 관리자 권한이 필요한 항목(Program Files/HKLM 등)이 있으면 승격 헬퍼로,
        // 아니면 인앱(STA)에서 삭제.
        DeleteResult delResult;
        if (ElevatedHelper.NeedsElevation(selected))
        {
            ViewModel.Status = $"관리자 권한으로 관련 파일 {selected.Count}개 삭제 중...";
            delResult = await Task.Run(() => ElevatedHelper.DeleteElevated(selected));
        }
        else
        {
            ViewModel.Status = $"관련 파일 {selected.Count}개 삭제 중...";
            // 휴지통 이동(trash/COM)은 STA 스레드에서.
            delResult = await StaRunner.RunAsync(() => CoreEngine.DeleteResidue(selected));
        }

        string summary = $"삭제됨 {delResult.Deleted.Count}개";
        if (delResult.Failed.Count > 0)
        {
            summary += $", 실패 {delResult.Failed.Count}개";
        }
        ViewModel.Status = $"관련 파일 정리 완료 — {summary}";

        // 실패(권한 부족 등)가 있으면 사유를 함께 보여준다.
        if (delResult.Failed.Count > 0)
        {
            string fails = string.Join("\n", delResult.Failed.Select(f => $"• {f.Path}\n   → {f.Error}"));
            var failDialog = new ContentDialog
            {
                XamlRoot = XamlRoot,
                Title = "일부 관련 파일 삭제 실패",
                Content = new ScrollViewer { Content = new TextBlock { Text = fails, TextWrapping = TextWrapping.Wrap } },
                CloseButtonText = "확인",
            };
            await failDialog.ShowAsync();
        }

        // 휴지통으로 보낸 항목이 있으면 영구삭제 옵션을 제안. (F6)
        if (delResult.RecycledPaths.Count > 0)
        {
            await OfferPermanentDeleteAsync(delResult.RecycledPaths);
        }
    }

    /// <summary>
    /// 잔여물을 휴지통으로 보낸 뒤, 영구삭제 여부를 묻는다. (F6)
    /// 기본은 휴지통 보관(복구 가능). 원하면 방금 항목만 영구삭제하거나 휴지통 전체를 비운다.
    /// </summary>
    private async Task OfferPermanentDeleteAsync(System.Collections.Generic.IReadOnlyList<string> recycledPaths)
    {
        var dialog = new ContentDialog
        {
            XamlRoot = XamlRoot,
            Title = "영구삭제 하시겠어요?",
            Content = $"잔여물 {recycledPaths.Count}개를 휴지통으로 보냈습니다(복구 가능).\n\n" +
                      "• 방금 지운 항목만 영구삭제: 다른 휴지통 파일은 그대로 둡니다.\n" +
                      "• 휴지통 전체 비우기: ⚠️ 다른 모든 휴지통 파일까지 영구삭제됩니다.",
            PrimaryButtonText = "방금 항목만 영구삭제",
            SecondaryButtonText = "휴지통 전체 비우기",
            CloseButtonText = "휴지통에 보관",
            DefaultButton = ContentDialogButton.Close,
        };

        ContentDialogResult choice = await dialog.ShowAsync();
        PurgeResult? purge = null;

        if (choice == ContentDialogResult.Primary)
        {
            purge = await StaRunner.RunAsync(() => CoreEngine.PermanentlyDelete(recycledPaths));
        }
        else if (choice == ContentDialogResult.Secondary)
        {
            // 전체 비우기는 한 번 더 확인.
            var reconfirm = new ContentDialog
            {
                XamlRoot = XamlRoot,
                Title = "휴지통 전체 비우기",
                Content = "휴지통의 모든 파일이 영구삭제됩니다. 계속할까요?",
                PrimaryButtonText = "전체 비우기",
                CloseButtonText = "취소",
                DefaultButton = ContentDialogButton.Close,
            };
            if (await reconfirm.ShowAsync() == ContentDialogResult.Primary)
            {
                purge = await StaRunner.RunAsync(CoreEngine.EmptyRecycleBin);
            }
        }

        if (purge is not null)
        {
            ViewModel.Status = purge.Message;
        }
    }

    // ===== 드래그앤드롭 (F3) =====

    /// <summary>드래그가 들어오면 어둠 오버레이를 띄운다.</summary>
    private void OnDragEnter(object sender, DragEventArgs e)
    {
        if (e.DataView.Contains(StandardDataFormats.StorageItems))
        {
            ShowDragOverlay("여기에 바로가기를 놓으세요", "해당 앱을 찾아 선택합니다", busy: false);
        }
    }

    /// <summary>드래그 중 — 파일이면 복사 동작 허용.</summary>
    private void OnDragOver(object sender, DragEventArgs e)
    {
        e.AcceptedOperation = e.DataView.Contains(StandardDataFormats.StorageItems)
            ? DataPackageOperation.Copy
            : DataPackageOperation.None;
        if (e.DragUIOverride is not null)
        {
            e.DragUIOverride.IsCaptionVisible = false;
        }
    }

    /// <summary>드래그가 영역을 벗어나면 오버레이를 숨긴다.</summary>
    private void OnDragLeave(object sender, DragEventArgs e) => HideDragOverlay();

    /// <summary>
    /// 바로가기(.lnk)를 놓으면 대상 exe를 추출 → 설치 앱 매칭 → 삭제 흐름 실행. (F3)
    /// 진행 상황은 어둠 오버레이로 시각적으로 안내한다.
    /// </summary>
    private async void OnDrop(object sender, DragEventArgs e)
    {
        if (!e.DataView.Contains(StandardDataFormats.StorageItems))
        {
            HideDragOverlay();
            return;
        }

        ShowDragOverlay("바로가기 확인 중...", "", busy: true);

        var deferral = e.GetDeferral();
        IReadOnlyList<IStorageItem> items;
        try
        {
            items = await e.DataView.GetStorageItemsAsync();
        }
        finally
        {
            deferral.Complete();
        }

        // 백업 파일(.appbak)을 끌어왔으면 복원 흐름으로. (Phase 7)
        string? bakPath = items
            .OfType<StorageFile>()
            .Select(f => f.Path)
            .FirstOrDefault(p => p.EndsWith(".appbak", StringComparison.OrdinalIgnoreCase));
        if (bakPath is not null)
        {
            HideDragOverlay();
            await RestoreFromArchiveAsync(bakPath);
            return;
        }

        // 끌어온 첫 번째 .lnk(또는 .exe)를 대상으로.
        string? lnkPath = items
            .OfType<StorageFile>()
            .Select(f => f.Path)
            .FirstOrDefault(p => p.EndsWith(".lnk", StringComparison.OrdinalIgnoreCase));

        string? target;
        if (lnkPath is not null)
        {
            target = ShortcutResolver.ResolveTarget(lnkPath);
        }
        else
        {
            // .exe를 직접 끌어온 경우도 허용.
            target = items.OfType<StorageFile>()
                .Select(f => f.Path)
                .FirstOrDefault(p => p.EndsWith(".exe", StringComparison.OrdinalIgnoreCase));
        }

        if (string.IsNullOrWhiteSpace(target))
        {
            await FlashOverlayThenHideAsync("대상을 찾지 못했습니다", "바로가기(.lnk) 또는 실행파일을 끌어다 놓아 주세요");
            ViewModel.Status = "바로가기에서 대상 프로그램을 찾지 못했습니다.";
            return;
        }

        // 대상 → 설치 앱 매칭.
        AppInfo? matched = await Task.Run(() => CoreEngine.MatchShortcut(target));

        if (matched is null)
        {
            await FlashOverlayThenHideAsync("일치하는 앱 없음", $"대상: {target}");
            ViewModel.Status = "일치하는 앱 없음";
            return;
        }

        // 인식 성공 → 목록에서 해당 앱을 '선택'만 한다(삭제는 사용자가 버튼으로).
        // 검색 필터가 걸려 있으면 풀어서 목록에 보이게 한다.
        ViewModel.SearchText = "";
        AppInfo selected = ViewModel.Apps.FirstOrDefault(a => a.Id == matched.Id) ?? matched;
        ViewModel.SelectedApp = selected;
        ShowDragOverlay(matched.Name, "선택했습니다 — 왼쪽에서 확인 후 '이 앱 삭제'를 누르세요", busy: false);
        DragOverlayIcon.Glyph = ""; // 체크마크
        await Task.Delay(900);
        HideDragOverlay();
        DragOverlayIcon.Glyph = ""; // 원복

        ViewModel.Status = $"'{matched.Name}' 선택됨";
    }

    /// <summary>
    /// .appbak 백업 파일을 미리보기로 확인한 뒤 이 PC에 복원한다. (Phase 7)
    /// </summary>
    private async Task RestoreFromArchiveAsync(string archivePath)
    {
        ViewModel.Status = "백업 파일 확인 중...";
        BackupManifest manifest = await Task.Run(() => CoreEngine.InspectBackup(archivePath));

        if (!string.IsNullOrEmpty(manifest.Error))
        {
            var err = new ContentDialog
            {
                XamlRoot = XamlRoot,
                Title = "백업 파일을 읽을 수 없습니다",
                Content = manifest.Error,
                CloseButtonText = "확인",
            };
            await err.ShowAsync();
            ViewModel.Status = "백업 파일 오류";
            return;
        }

        // 미리보기 + 덮어쓰기 옵션.
        var overwriteBox = new CheckBox { Content = "기존 파일 덮어쓰기", IsChecked = false, Margin = new Thickness(0, 8, 0, 0) };
        var folderLines = manifest.Entries.Select(en => $"• [{en.RootKind}] {en.OriginalPath}");
        var regLines = manifest.Registry.Select(rk => $"• [레지스트리] {rk.KeyPath}");
        var list = string.Join("\n", folderLines.Concat(regLines));
        var panel = new StackPanel { Spacing = 4 };
        panel.Children.Add(new TextBlock
        {
            Text = $"앱: {manifest.Name}\n원본 PC: {manifest.SourceMachine}\n생성: {manifest.CreatedAt}\n\n복원할 항목 {manifest.Entries.Count + manifest.Registry.Count}개:",
            TextWrapping = TextWrapping.Wrap,
        });
        panel.Children.Add(new TextBlock { Text = list, TextWrapping = TextWrapping.Wrap, FontSize = 12 });
        panel.Children.Add(overwriteBox);

        var confirm = new ContentDialog
        {
            XamlRoot = XamlRoot,
            Title = "백업 복원",
            Content = new ScrollViewer { Content = panel, MaxHeight = 360 },
            PrimaryButtonText = "복원",
            CloseButtonText = "취소",
            DefaultButton = ContentDialogButton.Close,
        };
        if (await confirm.ShowAsync() != ContentDialogResult.Primary)
        {
            ViewModel.Status = "복원을 취소했습니다.";
            return;
        }

        bool overwrite = overwriteBox.IsChecked == true;
        ViewModel.Status = "복원 중...";
        BackupRestoreResult result = await Task.Run(() => CoreEngine.RestoreBackup(archivePath, overwrite));
        ViewModel.Status = result.Message;

        var done = new ContentDialog
        {
            XamlRoot = XamlRoot,
            Title = result.Success ? "복원 완료" : "복원 결과",
            Content = result.Message,
            CloseButtonText = "확인",
        };
        await done.ShowAsync();
    }

    private void ShowDragOverlay(string title, string subtitle, bool busy)
    {
        DragOverlayTitle.Text = title;
        DragOverlaySubtitle.Text = subtitle;
        DragOverlayRing.IsActive = busy;
        DragOverlay.Visibility = Visibility.Visible;
    }

    private void HideDragOverlay()
    {
        DragOverlay.Visibility = Visibility.Collapsed;
        DragOverlayRing.IsActive = false;
    }

    private async Task FlashOverlayThenHideAsync(string title, string subtitle)
    {
        ShowDragOverlay(title, subtitle, busy: false);
        await Task.Delay(1200);
        HideDragOverlay();
    }
}

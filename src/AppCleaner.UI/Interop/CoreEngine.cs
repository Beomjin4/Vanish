using System;
using System.Collections.Generic;
using System.Linq;
using System.Runtime.InteropServices;
using System.Text;
using System.Text.Json;
using AppCleaner_UI.Models;

namespace AppCleaner_UI.Interop;

/// <summary>
/// Rust 코어 엔진(core_engine.dll)에 대한 P/Invoke 바인딩.
/// 문자열은 UTF-8 C 문자열로 주고받으며, Rust가 반환한 포인터는
/// 사용 직후 <c>free_string</c>으로 반드시 해제한다.
/// </summary>
internal static class CoreEngine
{
    private const string Dll = "core_engine";

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl, EntryPoint = "ping")]
    private static extern IntPtr PingNative(byte[]? name);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl, EntryPoint = "scan_installed_apps")]
    private static extern IntPtr ScanInstalledAppsNative();

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl, EntryPoint = "uninstall_app")]
    private static extern IntPtr UninstallAppNative(byte[] requestJson);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl, EntryPoint = "find_residue")]
    private static extern IntPtr FindResidueNative(byte[] requestJson);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl, EntryPoint = "delete_residue")]
    private static extern IntPtr DeleteResidueNative(byte[] requestJson);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl, EntryPoint = "permanently_delete")]
    private static extern IntPtr PermanentlyDeleteNative(byte[] requestJson);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl, EntryPoint = "empty_recycle_bin")]
    private static extern IntPtr EmptyRecycleBinNative();

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl, EntryPoint = "match_shortcut")]
    private static extern IntPtr MatchShortcutNative(byte[] targetExe);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl, EntryPoint = "create_backup")]
    private static extern IntPtr CreateBackupNative(byte[] requestJson);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl, EntryPoint = "inspect_backup")]
    private static extern IntPtr InspectBackupNative(byte[] archivePath);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl, EntryPoint = "restore_backup")]
    private static extern IntPtr RestoreBackupNative(byte[] requestJson);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl, EntryPoint = "free_string")]
    private static extern void FreeString(IntPtr ptr);

    private static readonly JsonSerializerOptions JsonOpts = new()
    {
        PropertyNamingPolicy = JsonNamingPolicy.SnakeCaseLower,
    };

    /// <summary>
    /// FFI 왕복 검증용. 이름을 넘기면 Rust가 인사 문자열을 만들어 돌려준다.
    /// </summary>
    public static string Ping(string name)
    {
        // C# string -> UTF-8 + NUL 종단 바이트 배열
        byte[] utf8 = Encoding.UTF8.GetBytes(name + '\0');

        IntPtr ptr = PingNative(utf8);
        try
        {
            return PtrToUtf8String(ptr);
        }
        finally
        {
            // Rust가 할당한 메모리를 Rust 쪽 free로 해제 (CRT 불일치 방지)
            FreeString(ptr);
        }
    }

    /// <summary>
    /// 설치된 앱 목록을 스캔해 반환한다. (레지스트리 3곳 조회)
    /// </summary>
    public static List<AppInfo> ScanInstalledApps()
    {
        IntPtr ptr = ScanInstalledAppsNative();
        try
        {
            string json = PtrToUtf8String(ptr);
            return JsonSerializer.Deserialize<List<AppInfo>>(json, JsonOpts) ?? new List<AppInfo>();
        }
        finally
        {
            FreeString(ptr);
        }
    }

    /// <summary>
    /// 정식 언인스톨러를 실행해 앱을 제거한다. (Phase 2)
    /// 언인스톨러(대개 GUI 마법사)가 끝날 때까지 블로킹되므로 백그라운드 스레드에서 호출할 것.
    /// </summary>
    public static UninstallResult UninstallApp(AppInfo app)
    {
        var request = new { uninstall_string = app.UninstallString, name = app.Name };
        string requestJson = JsonSerializer.Serialize(request);
        byte[] utf8 = Encoding.UTF8.GetBytes(requestJson + '\0');

        IntPtr ptr = UninstallAppNative(utf8);
        try
        {
            string json = PtrToUtf8String(ptr);
            return JsonSerializer.Deserialize<UninstallResult>(json, JsonOpts)
                   ?? new UninstallResult { Success = false, Message = "응답 없음" };
        }
        finally
        {
            FreeString(ptr);
        }
    }

    /// <summary>
    /// 정식 제거 후 남은 잔여물(폴더/레지스트리) 후보를 찾는다. (Phase 3, 읽기 전용)
    /// </summary>
    public static List<ResidueItem> FindResidue(AppInfo app)
    {
        var request = new
        {
            name = app.Name,
            publisher = app.Publisher,
            install_location = app.InstallLocation,
        };
        byte[] utf8 = Encoding.UTF8.GetBytes(JsonSerializer.Serialize(request) + '\0');

        IntPtr ptr = FindResidueNative(utf8);
        try
        {
            string json = PtrToUtf8String(ptr);
            return JsonSerializer.Deserialize<List<ResidueItem>>(json, JsonOpts) ?? new List<ResidueItem>();
        }
        finally
        {
            FreeString(ptr);
        }
    }

    /// <summary>
    /// 선택된 잔여물만 삭제한다(폴더→휴지통, 레지스트리→삭제). (Phase 3)
    /// </summary>
    public static DeleteResult DeleteResidue(IEnumerable<ResidueItem> items)
    {
        // 직렬화 시 Rust가 기대하는 필드(path/kind/size_mb)만 보낸다.
        var payload = new
        {
            items = items.Select(i => new { path = i.Path, kind = i.Kind, size_mb = i.SizeMb }),
        };
        byte[] utf8 = Encoding.UTF8.GetBytes(JsonSerializer.Serialize(payload) + '\0');

        IntPtr ptr = DeleteResidueNative(utf8);
        try
        {
            string json = PtrToUtf8String(ptr);
            return JsonSerializer.Deserialize<DeleteResult>(json, JsonOpts) ?? new DeleteResult();
        }
        finally
        {
            FreeString(ptr);
        }
    }

    /// <summary>휴지통에서 추적된 항목(원본 경로)만 영구삭제한다. (F6)</summary>
    public static PurgeResult PermanentlyDelete(IEnumerable<string> paths)
    {
        var payload = new { paths };
        byte[] utf8 = Encoding.UTF8.GetBytes(JsonSerializer.Serialize(payload) + '\0');
        IntPtr ptr = PermanentlyDeleteNative(utf8);
        try
        {
            return JsonSerializer.Deserialize<PurgeResult>(PtrToUtf8String(ptr), JsonOpts)
                   ?? new PurgeResult();
        }
        finally
        {
            FreeString(ptr);
        }
    }

    /// <summary>휴지통 전체를 비운다. (⚠️ 다른 모든 휴지통 파일도 영구삭제) (F6)</summary>
    public static PurgeResult EmptyRecycleBin()
    {
        IntPtr ptr = EmptyRecycleBinNative();
        try
        {
            return JsonSerializer.Deserialize<PurgeResult>(PtrToUtf8String(ptr), JsonOpts)
                   ?? new PurgeResult();
        }
        finally
        {
            FreeString(ptr);
        }
    }

    /// <summary>바로가기 대상 exe 경로로 설치 앱을 찾는다. 없으면 null. (F3)</summary>
    public static AppInfo? MatchShortcut(string targetExe)
    {
        byte[] utf8 = Encoding.UTF8.GetBytes(targetExe + '\0');
        IntPtr ptr = MatchShortcutNative(utf8);
        try
        {
            string json = PtrToUtf8String(ptr);
            if (string.IsNullOrWhiteSpace(json) || json == "null")
            {
                return null;
            }
            return JsonSerializer.Deserialize<AppInfo>(json, JsonOpts);
        }
        finally
        {
            FreeString(ptr);
        }
    }

    /// <summary>앱 데이터 폴더 + 레지스트리 키를 .appbak으로 백업한다. (Phase 7)</summary>
    public static BackupCreateResult CreateBackup(
        AppInfo app, IEnumerable<string> folderPaths, IEnumerable<string> registryKeys, string outputPath)
    {
        var request = new
        {
            name = app.Name,
            publisher = app.Publisher,
            version = app.Version,
            paths = folderPaths,
            registry_keys = registryKeys,
            output_path = outputPath,
        };
        byte[] utf8 = Encoding.UTF8.GetBytes(JsonSerializer.Serialize(request) + '\0');
        IntPtr ptr = CreateBackupNative(utf8);
        try
        {
            return JsonSerializer.Deserialize<BackupCreateResult>(PtrToUtf8String(ptr), JsonOpts)
                   ?? new BackupCreateResult();
        }
        finally
        {
            FreeString(ptr);
        }
    }

    /// <summary>.appbak에서 manifest만 읽어 미리보기. (Phase 7)</summary>
    public static BackupManifest InspectBackup(string archivePath)
    {
        byte[] utf8 = Encoding.UTF8.GetBytes(archivePath + '\0');
        IntPtr ptr = InspectBackupNative(utf8);
        try
        {
            return JsonSerializer.Deserialize<BackupManifest>(PtrToUtf8String(ptr), JsonOpts)
                   ?? new BackupManifest { Error = "응답 없음" };
        }
        finally
        {
            FreeString(ptr);
        }
    }

    /// <summary>.appbak을 이 PC에 복원한다. (Phase 7)</summary>
    public static BackupRestoreResult RestoreBackup(string archivePath, bool overwrite)
    {
        var request = new { archive_path = archivePath, overwrite };
        byte[] utf8 = Encoding.UTF8.GetBytes(JsonSerializer.Serialize(request) + '\0');
        IntPtr ptr = RestoreBackupNative(utf8);
        try
        {
            return JsonSerializer.Deserialize<BackupRestoreResult>(PtrToUtf8String(ptr), JsonOpts)
                   ?? new BackupRestoreResult();
        }
        finally
        {
            FreeString(ptr);
        }
    }

    /// <summary>UTF-8 NUL 종단 C 문자열 포인터를 C# string으로 변환.</summary>
    private static string PtrToUtf8String(IntPtr ptr)
    {
        if (ptr == IntPtr.Zero)
        {
            return string.Empty;
        }

        // NUL 종단까지 길이 측정
        int len = 0;
        while (Marshal.ReadByte(ptr, len) != 0)
        {
            len++;
        }

        byte[] buffer = new byte[len];
        Marshal.Copy(ptr, buffer, 0, len);
        return Encoding.UTF8.GetString(buffer);
    }
}

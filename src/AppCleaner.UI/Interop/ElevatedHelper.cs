using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Text.Json;
using AppCleaner_UI.Models;

namespace AppCleaner_UI.Interop;

/// <summary>
/// 관리자 권한이 필요한 잔여물 삭제를 별도 헬퍼 exe(appcleaner_helper.exe)로
/// ShellExecute("runas") 승격 실행한다. 메인 앱은 일반권한을 유지(드래그앤드롭 보존),
/// 삭제 작업만 그때그때 승격한다("삭제때만 승격" 정책).
/// </summary>
internal static class ElevatedHelper
{
    private static readonly JsonSerializerOptions JsonOpts = new()
    {
        PropertyNamingPolicy = JsonNamingPolicy.SnakeCaseLower,
    };

    /// <summary>이 항목들을 지우려면 관리자 권한이 필요한가?</summary>
    public static bool NeedsElevation(IEnumerable<ResidueItem> items)
    {
        foreach (var it in items)
        {
            if (it.Kind == "registry" &&
                it.Path.StartsWith("HKLM", StringComparison.OrdinalIgnoreCase))
            {
                return true;
            }
            if (it.Kind == "folder")
            {
                string p = it.Path.ToLowerInvariant();
                if (p.Contains(@"\program files") ||
                    p.StartsWith(@"c:\programdata") ||
                    p.Contains(@"\windows\"))
                {
                    return true;
                }
            }
        }
        return false;
    }

    /// <summary>헬퍼를 승격 실행해 잔여물을 삭제하고 결과를 받는다.</summary>
    public static DeleteResult DeleteElevated(IEnumerable<ResidueItem> items)
    {
        var list = items.ToList();
        string dir = Path.Combine(Path.GetTempPath(), "AppCleaner");
        Directory.CreateDirectory(dir);
        string token = Guid.NewGuid().ToString("N");
        string inFile = Path.Combine(dir, $"del_{token}.json");
        string outFile = Path.Combine(dir, $"del_{token}.out.json");

        var payload = new
        {
            items = list.Select(i => new { path = i.Path, kind = i.Kind, size_mb = i.SizeMb }),
        };
        File.WriteAllText(inFile, JsonSerializer.Serialize(payload));

        string helper = Path.Combine(AppContext.BaseDirectory, "appcleaner_helper.exe");
        if (!File.Exists(helper))
        {
            return Fail("삭제 헬퍼(appcleaner_helper.exe)를 찾을 수 없습니다.");
        }

        var psi = new ProcessStartInfo
        {
            FileName = helper,
            Arguments = $"\"{inFile}\" \"{outFile}\"",
            UseShellExecute = true,   // runas 동사 사용에 필요
            Verb = "runas",           // UAC 승격
            WindowStyle = ProcessWindowStyle.Hidden,
            CreateNoWindow = true,
        };

        try
        {
            using var proc = Process.Start(psi);
            proc?.WaitForExit();
        }
        catch (Exception ex)
        {
            // 사용자가 UAC를 취소하면 여기로 온다.
            return Fail("관리자 권한 실행이 취소/실패했습니다: " + ex.Message);
        }

        if (!File.Exists(outFile))
        {
            return Fail("삭제 결과를 받지 못했습니다(권한 거부 또는 취소).");
        }

        try
        {
            string json = File.ReadAllText(outFile);
            var result = JsonSerializer.Deserialize<DeleteResult>(json, JsonOpts) ?? new DeleteResult();
            TryCleanup(inFile, outFile);
            return result;
        }
        catch (Exception ex)
        {
            TryCleanup(inFile, outFile);
            return Fail("결과 파싱 실패: " + ex.Message);
        }
    }

    private static DeleteResult Fail(string message) =>
        new() { Failed = { new FailedItem { Path = "", Error = message } } };

    private static void TryCleanup(params string[] files)
    {
        foreach (var f in files)
        {
            try { File.Delete(f); } catch { /* ignore */ }
        }
    }
}

using System;
using System.Runtime.InteropServices;

namespace AppCleaner_UI.Interop;

/// <summary>
/// .lnk 바로가기 파일에서 대상 exe 경로를 추출한다. (드래그앤드롭, F3)
/// Windows Script Host의 IWshShortcut COM(WScript.Shell)을 늦은 바인딩으로 사용한다.
/// </summary>
internal static class ShortcutResolver
{
    /// <summary>.lnk 경로 → 대상 exe 절대경로. 실패 시 null.</summary>
    public static string? ResolveTarget(string lnkPath)
    {
        object? shell = null;
        object? shortcut = null;
        try
        {
            Type? shellType = Type.GetTypeFromProgID("WScript.Shell");
            if (shellType is null)
            {
                return null;
            }

            shell = Activator.CreateInstance(shellType);
            // shell.CreateShortcut(lnkPath) → IWshShortcut
            shortcut = shellType.InvokeMember(
                "CreateShortcut",
                System.Reflection.BindingFlags.InvokeMethod,
                null, shell, new object[] { lnkPath });

            if (shortcut is null)
            {
                return null;
            }

            object? target = shortcut.GetType().InvokeMember(
                "TargetPath",
                System.Reflection.BindingFlags.GetProperty,
                null, shortcut, null);

            string? path = target as string;
            return string.IsNullOrWhiteSpace(path) ? null : path;
        }
        catch
        {
            return null;
        }
        finally
        {
            if (shortcut is not null && Marshal.IsComObject(shortcut))
            {
                Marshal.ReleaseComObject(shortcut);
            }
            if (shell is not null && Marshal.IsComObject(shell))
            {
                Marshal.ReleaseComObject(shell);
            }
        }
    }
}

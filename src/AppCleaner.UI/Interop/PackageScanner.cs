using System;
using System.Collections.Generic;
using System.Threading.Tasks;
using Windows.ApplicationModel;
using Windows.Management.Deployment;
// AppInfo는 우리 모델을 가리키도록 별칭 지정(Windows.ApplicationModel.AppInfo와 충돌 방지).
using AppInfo = AppCleaner_UI.Models.AppInfo;
using UninstallResult = AppCleaner_UI.Models.UninstallResult;

namespace AppCleaner_UI.Interop;

/// <summary>
/// MSIX/APPX(Store·패키지 앱) 열거·제거. 레지스트리가 아니라 WinRT PackageManager로 다룬다.
/// 패키지 앱은 데이터가 %LocalAppData%\Packages\&lt;PackageFamilyName&gt; 에 모인다.
/// </summary>
internal static class PackageScanner
{
    /// <summary>현재 사용자에게 설치된 사용자용 패키지 앱을 AppInfo로 반환.</summary>
    public static List<AppInfo> Scan()
    {
        var list = new List<AppInfo>();

        PackageManager pm;
        IEnumerable<Package> packages;
        try
        {
            pm = new PackageManager();
            packages = pm.FindPackagesForUser(string.Empty); // 현재 사용자
        }
        catch
        {
            return list; // 권한/환경 문제 시 빈 목록
        }

        foreach (var pkg in packages)
        {
            try
            {
                // 프레임워크/리소스/번들 보조 패키지는 사용자에게 안 보여줌.
                if (pkg.IsFramework || pkg.IsResourcePackage)
                {
                    continue;
                }

                string display = Safe(() => pkg.DisplayName);
                // 이름이 비었거나 미해석 리소스 문자열이면 제외(시스템 잡음).
                if (string.IsNullOrWhiteSpace(display) || display.StartsWith("ms-resource:", StringComparison.OrdinalIgnoreCase))
                {
                    continue;
                }

                string installPath = Safe(() => pkg.InstalledLocation?.Path ?? "");
                var v = pkg.Id.Version;

                var app = new AppInfo
                {
                    Id = pkg.Id.FullName,
                    Name = display,
                    Publisher = Safe(() => pkg.PublisherDisplayName),
                    Version = $"{v.Major}.{v.Minor}.{v.Build}.{v.Revision}",
                    InstallLocation = installPath,
                    Source = "MSIX",
                    IsPackaged = true,
                    PackageFullName = pkg.Id.FullName,
                    PackageFamilyName = pkg.Id.FamilyName,
                };
                try { app.LogoUri = pkg.Logo; } catch { /* 로고 없을 수 있음 */ }

                list.Add(app);
            }
            catch
            {
                // 문제 있는 패키지는 건너뜀.
            }
        }

        return list;
    }

    /// <summary>패키지 앱을 제거한다(현재 사용자).</summary>
    public static async Task<UninstallResult> RemoveAsync(string packageFullName)
    {
        try
        {
            var pm = new PackageManager();
            DeploymentResult result = await pm.RemovePackageAsync(packageFullName, RemovalOptions.None);

            if (result.ExtendedErrorCode is not null)
            {
                return new UninstallResult
                {
                    Success = false,
                    Message = $"패키지 제거 실패: {result.ErrorText}",
                };
            }

            return new UninstallResult
            {
                Success = true,
                Message = "패키지 앱을 제거했습니다.",
            };
        }
        catch (Exception ex)
        {
            return new UninstallResult
            {
                Success = false,
                Message = $"패키지 제거 실패: {ex.Message}",
            };
        }
    }

    private static string Safe(Func<string> getter)
    {
        try { return getter() ?? ""; }
        catch { return ""; }
    }
}

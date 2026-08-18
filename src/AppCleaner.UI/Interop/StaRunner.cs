using System;
using System.Threading;
using System.Threading.Tasks;

namespace AppCleaner_UI.Interop;

/// <summary>
/// 작업을 전용 STA 스레드에서 실행한다.
/// trash(휴지통, IFileOperation)·ShellExecute 등 COM 기반 작업은
/// .NET 스레드풀(MTA)에서 호출하면 COM 아파트먼트 충돌로 실패/크래시할 수 있어,
/// STA 스레드에서 돌려야 안전하다.
/// </summary>
internal static class StaRunner
{
    public static Task<T> RunAsync<T>(Func<T> func)
    {
        var tcs = new TaskCompletionSource<T>();
        var thread = new Thread(() =>
        {
            try
            {
                tcs.SetResult(func());
            }
            catch (Exception ex)
            {
                tcs.SetException(ex);
            }
        })
        {
            IsBackground = true,
            Name = "AppCleaner-STA",
        };
        thread.SetApartmentState(ApartmentState.STA);
        thread.Start();
        return tcs.Task;
    }
}

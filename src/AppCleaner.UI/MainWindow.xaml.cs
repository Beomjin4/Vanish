using System;
using System.Runtime.InteropServices;
using Microsoft.UI.Xaml;

// To learn more about WinUI, the WinUI project structure,
// and more about our project templates, see: http://aka.ms/winui-project-info.

namespace AppCleaner_UI;

/// <summary>
/// The application window. This hosts a Frame that displays pages. Add your
/// UI and logic to MainPage.xaml / MainPage.xaml.cs instead of here so you
/// can use Page features such as navigation events and the Loaded lifecycle.
/// </summary>
public sealed partial class MainWindow : Window
{
    public MainWindow()
    {
        InitializeComponent();

        ExtendsContentIntoTitleBar = true;
        SetTitleBar(AppTitleBar);

        // 설치 후 바로가기로 실행하면 작업 디렉터리가 달라질 수 있으니 절대경로로.
        AppWindow.SetIcon(System.IO.Path.Combine(AppContext.BaseDirectory, "Assets", "AppIcon.ico"));

        // 관리자 권한 실행 시 UIPI가 일반 권한(바탕화면)→관리자 창 드래그앤드롭을 막는다.
        // 드롭 관련 메시지를 화이트리스트에 추가해 드래그앤드롭(F3)을 허용한다.
        EnableDragDropForElevated();

        // Navigate the root frame to the main page on startup.
        RootFrame.Navigate(typeof(MainPage));
    }

    private void EnableDragDropForElevated()
    {
        try
        {
            IntPtr hwnd = WinRT.Interop.WindowNative.GetWindowHandle(this);
            // WM_DROPFILES, WM_COPYDATA, WM_COPYGLOBALDATA
            foreach (uint msg in new uint[] { 0x0233, 0x004A, 0x0049 })
            {
                ChangeWindowMessageFilterEx(hwnd, msg, MSGFLT_ALLOW, IntPtr.Zero);
            }
        }
        catch
        {
            // 실패해도 앱 자체는 동작 — 드래그앤드롭만 제한될 수 있음.
        }
    }

    private const uint MSGFLT_ALLOW = 1;

    [DllImport("user32.dll", SetLastError = true)]
    private static extern bool ChangeWindowMessageFilterEx(IntPtr hwnd, uint message, uint action, IntPtr pChangeFilterStruct);
}

; Vanish 인스톨러 (Inno Setup)
; self-contained 퍼블리시 결과(dist\Vanish)를 묶어 per-user 설치한다.

#define MyAppName "Vanish"
#define MyAppVersion "1.0.0"
#define MyAppPublisher "Vanish"
#define MyAppExe "Vanish.exe"

[Setup]
AppId={{8F3A1C2E-9B7D-4E55-AAF1-2C6D9E0B7A41}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
DefaultDirName={autopf}\{#MyAppName}
DefaultGroupName={#MyAppName}
DisableProgramGroupPage=yes
UninstallDisplayIcon={app}\{#MyAppExe}
UninstallDisplayName={#MyAppName}
OutputDir=..\dist
OutputBaseFilename=VanishSetup
Compression=lzma2/max
SolidCompression=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
WizardStyle=modern
SetupIconFile=..\src\AppCleaner.UI\Assets\AppIcon.ico

[Files]
Source: "..\dist\Vanish\*"; DestDir: "{app}"; Flags: recursesubdirs createallsubdirs ignoreversion

[Icons]
Name: "{autoprograms}\{#MyAppName}"; Filename: "{app}\{#MyAppExe}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExe}"

[Run]
Filename: "{app}\{#MyAppExe}"; Description: "{#MyAppName} 실행"; Flags: nowait postinstall skipifsilent

; Inno Setup script for the Windows installer. Build it with
; scripts/windows/build_installer.ps1, which passes AppVersion and StageDir and
; sets the output folder and file name.

#ifndef AppVersion
  #error AppVersion must be defined
#endif
#ifndef StageDir
  #error StageDir must be defined
#endif

[Setup]
AppId={{6F0E4C1B-6E2A-4F5B-9C1D-2B7A9E3F8D41}
AppName=zBase
AppVersion={#AppVersion}
AppPublisher=zBase
AppPublisherURL=https://github.com/cameroncooper/zbase
DefaultDirName={localappdata}\Programs\zBase
DefaultGroupName=zBase
DisableProgramGroupPage=yes
; Per-user install: no administrator prompt.
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
SetupIconFile={#SourcePath}\..\..\assets\windows\zBase.ico
UninstallDisplayIcon={app}\zbase.exe
UninstallDisplayName=zBase
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
CloseApplications=yes

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#StageDir}\zbase.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#StageDir}\*.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#StageDir}\licenses\*"; DestDir: "{app}\licenses"; Flags: ignoreversion recursesubdirs
Source: "{#StageDir}\MicrosoftEdgeWebview2Setup.exe"; DestDir: "{tmp}"; Flags: deleteafterinstall; Check: NeedsWebView2

[Icons]
Name: "{autoprograms}\zBase"; Filename: "{app}\zbase.exe"
Name: "{autodesktop}\zBase"; Filename: "{app}\zbase.exe"; Tasks: desktopicon

[Run]
; zBase renders in WebView2, which Windows 11 includes and Windows 10 may not.
Filename: "{tmp}\MicrosoftEdgeWebview2Setup.exe"; Parameters: "/silent /install"; StatusMsg: "Installing Microsoft Edge WebView2 Runtime..."; Check: NeedsWebView2; Flags: waituntilterminated
Filename: "{app}\zbase.exe"; Description: "{cm:LaunchProgram,zBase}"; Flags: nowait postinstall skipifsilent

[Code]
const
  WebView2Key = 'Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}';

function HasWebView2(Root: Integer; Key: String): Boolean;
var
  Version: String;
begin
  Result := RegQueryStringValue(Root, Key, 'pv', Version) and (Version <> '') and (Version <> '0.0.0.0');
end;

{ Microsoft's documented detection: a per-machine or per-user runtime version. }
function NeedsWebView2: Boolean;
begin
  Result := not (HasWebView2(HKLM, 'SOFTWARE\WOW6432Node\' + WebView2Key)
    or HasWebView2(HKLM, 'SOFTWARE\' + WebView2Key)
    or HasWebView2(HKCU, 'Software\' + WebView2Key));
end;

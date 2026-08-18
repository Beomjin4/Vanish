using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace AppCleaner_UI.Models;

/// <summary>백업 아카이브(.appbak)의 manifest. Rust Manifest와 매핑.</summary>
public sealed class BackupManifest
{
    public string Format { get; set; } = "";
    public string Name { get; set; } = "";
    public string Publisher { get; set; } = "";
    public string Version { get; set; } = "";
    public string SourceMachine { get; set; } = "";
    public string CreatedAt { get; set; } = "";
    public List<BackupEntry> Entries { get; set; } = new();
    public List<BackupRegEntry> Registry { get; set; } = new();

    /// <summary>inspect 실패 시 오류 메시지(JSON에 error 필드가 있으면 채워짐).</summary>
    public string? Error { get; set; }
}

public sealed class BackupEntry
{
    public string RootKind { get; set; } = "";
    public string RelativePath { get; set; } = "";
    public string OriginalPath { get; set; } = "";
}

public sealed class BackupRegEntry
{
    public string KeyPath { get; set; } = "";
    public string ArchiveName { get; set; } = "";
}

public sealed class BackupCreateResult
{
    public bool Success { get; set; }
    public string Message { get; set; } = "";
    public int EntryCount { get; set; }
}

public sealed class BackupRestoreResult
{
    public bool Success { get; set; }
    public string Message { get; set; } = "";
    public int RestoredEntries { get; set; }
}

@echo off
setlocal
cd /d "%~dp0"
title CryptBench
set "CRYPTBENCH_LAUNCHER=%~f0"
powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -Command "$s=[IO.File]::ReadAllText($env:CRYPTBENCH_LAUNCHER); $marker='# POWERSHELL-BEGIN'; $i=$s.LastIndexOf($marker); if ($i -lt 0) { [Console]::Error.WriteLine('Launcher menu missing.'); exit 1 }; & ([scriptblock]::Create($s.Substring($i+$marker.Length)))"
set "CRYPTBENCH_EXIT=%ERRORLEVEL%"
if not "%CRYPTBENCH_EXIT%"=="0" (
    echo.
    pause
)
exit /b %CRYPTBENCH_EXIT%
# POWERSHELL-BEGIN
$ErrorActionPreference = 'Stop'
try {
    if ([Console]::IsInputRedirected -or [Console]::IsOutputRedirected) {
        throw 'The interactive menu requires a terminal.'
    }
    $buildJob = Start-Job -ScriptBlock {
        $sdkRoot = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\Lib'
        $sdk = Get-ChildItem -LiteralPath $sdkRoot -Directory | Sort-Object Name -Descending | Where-Object {
            (Test-Path -LiteralPath (Join-Path $_.FullName 'um\x64\dbghelp.lib')) -and
            (Test-Path -LiteralPath (Join-Path $_.FullName 'ucrt\x64\ucrt.lib'))
        } | Select-Object -First 1 -ExpandProperty Name
        if (-not $sdk) { throw 'A Windows SDK containing dbghelp.lib and ucrt.lib was not found.' }
        $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
        if (-not (Test-Path -LiteralPath $vswhere)) { throw 'Visual Studio C++ build tools were not found.' }
        $vs = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
        if (-not $vs) { throw 'Visual Studio C++ build tools were not found.' }
        $devcmd = Join-Path $vs 'Common7\Tools\VsDevCmd.bat'
        $environment = & cmd.exe /d /c "call `"$devcmd`" -no_logo -arch=x64 -host_arch=x64 -winsdk=$sdk >nul && set"
        if ($LASTEXITCODE -ne 0) { throw 'Could not prepare the Visual Studio build environment.' }
        $environment
    }
    function Select-Item([string]$title, [object[]]$items, [int]$initial = 0, [bool]$grid = $false) {
        $selected = $initial
        $originalColor = [Console]::ForegroundColor
        $originalCursor = [Console]::CursorVisible
        $redraw = $true
        try {
            [Console]::CursorVisible = $false
            while ($true) {
                if ($redraw) {
                $positions = @{}
                [Console]::Clear()
                <# Previous colour banner; keep for easy restoration.
                $banner = @(
                    '\e[0;36m▀\e[0;96m▀\e[0;97m▀\e[0;96m▀\e[0;36m▀\e[0;37m \e[0;36m▀\e[0;96m▀\e[0;97m▀\e[0;96m▀\e[0;36m▀\e[0;37m \e[0;96;46m▄\e[0;96m   \e[0;96;46m▄\e[0;37m \e[0;36m▀\e[0;96m▀\e[0;97m▀▄\e[0;96m \e[0;37m \e[0;36m▀\e[0;96m▀\e[0;97m▀▄\e[0;96m \e[0;37m \e[0;36m▀\e[0;96m▀\e[0;97m▀▄\e[0;96m \e[0;37m \e[0;36m▀\e[0;96m▀\e[0;97m▀\e[0;96m▀\e[0;36m▀\e[0;37m \e[0;36m▀\e[0;96m▀\e[0;97m▀▄\e[0;96m \e[0;37m \e[0;36m▀\e[0;96m▀\e[0;97m▀\e[0;96m▀\e[0;36m▀\e[0;37m \e[0;96;46m▄\e[0;37m   \e[0;96;46m▄\e[0m',
                    '\e[0;96;46m▄\e[0;96m    \e[0;37m \e[0;97m█\e[0;37m \e[0;36m▄\e[0;97m▄\e[0;96;46m▄\e[0;37m \e[0;96m \e[0;97m▀▄▄█\e[0;37m \e[0;97m█\e[0;96m \e[0;36m▄\e[0;96m▄\e[0;97m█\e[0;37m \e[0;96m    \e[0;97m█\e[0;37m \e[0;36m▄\e[0;96m \e[0;36m▀\e[0;96m▀\e[0;97m█\e[0;37m \e[0;96;46m▄\e[0;96m \e[0;97m▀\e[0;36m▀\e[0;96m \e[0;37m \e[0;96;46m▄\e[0;96m   \e[0;97m█\e[0;37m \e[0;96;46m▄\e[0;96m    \e[0;37m \e[0;97m█▀▀▀█\e[0m',
                    '\e[0;96m \e[0;97m▀▄\e[0;96m▄\e[0;36m▄\e[0;37m \e[0;96;46m▀\e[0;37m  \e[0;96m▀\e[0;36m▄\e[0;37m \e[0;36m▄\e[0;96m▄\e[0;97m▄▄▀\e[0;37m \e[0;96;46m▀\e[0;96m    \e[0;37m \e[0;96m    \e[0;96;46m▀\e[0;37m \e[0;97;47m▄\e[0;97m▄▄▄▀\e[0;37m \e[0;96m \e[0;97m▀▄▄\e[0;36m▄\e[0;37m \e[0;97;46m▀\e[0;96m   \e[0;96;46m▀\e[0;37m \e[0;96m \e[0;97m▀▄\e[0;96m▄\e[0;36m▄\e[0;37m \e[0;96;46m▀\e[0;37m   \e[0;96;46m▀\e[0m'
                )
                #>
                $banner = @(
                    '  ,ad8888ba,',
                    ' d8"''    `"8b                                         ,d',
                    'd8''                                                   88',
                    '88             8b,dPPYba,  8b       d8  8b,dPPYba,  MM88MMM',
                    '88             88P''   "Y8  `8b     d8''  88P''    "8a   88',
                    'Y8,            88           `8b   d8''   88       d8   88',
                    ' Y8a.    .a8P  88            `8b,d8''    88b,   ,a8"   88,',
                    '  `"Y8888Y"''   88              Y88''     88`YbbdP"''    "Y888',
                    '                               d8''      88',
                    '88888888ba                    d8''       88        88',
                    '88      "8b                                       88',
                    '88      ,8P                                       88',
                    '88aaaaaa8P''   ,adPPYba,  8b,dPPYba,    ,adPPYba,  88,dPPYba,',
                    '88""""""8b,  a8P_____88  88P''   `"8a  a8"     ""  88P''    "8a',
                    '88      `8b  8PP"""""""  88       88  8b          88       88',
                    '88      a8P  "8b,   ,aa  88       88  "8a,   ,aa  88       88',
                    '88888888P"    `"Ybbd8"''  88       88   `"Ybbd8"''  88       88'
                )
                $bannerWidth = ($banner | ForEach-Object { ([regex]::Replace($_, '\\e\[[0-9;]*m', '')).Length } | Measure-Object -Maximum).Maximum
                [Console]::ForegroundColor = [ConsoleColor]::White
                [Console]::WriteLine()
                for ($lineIndex = 0; $lineIndex -lt $banner.Count; $lineIndex++) {
                    $bannerLine = $banner[$lineIndex]
                    [Console]::WriteLine('        ' + $bannerLine)
                }
                [Console]::WriteLine()
                if ($grid) { [Console]::WriteLine() }
                if ($title) {
                    [Console]::WriteLine('    ' + $title)
                    [Console]::WriteLine()
                }
                $maxArrowOffset = 2
                if ($grid) {
                    $cellWidth = 26
                    $gridStart = [Math]::Max(0, 8 + [int][Math]::Floor(($bannerWidth - 3 * $cellWidth) / 2))
                    [Console]::ForegroundColor = [ConsoleColor]::White
                    [Console]::Write(' ' * $gridStart)
                    $headings = @('Password hashing', 'Hashing', 'Cipher')
                    [Console]::Write([string][char]27 + '[38;2;255;199;26m')
                    for ($headingIndex = 0; $headingIndex -lt $headings.Count; $headingIndex++) {
                        $heading = $headings[$headingIndex]
                        $before = [int][Math]::Floor(($cellWidth - $heading.Length) / 2)
                        $after = if ($headingIndex -eq $headings.Count - 1) { 0 } else { $cellWidth - $before - $heading.Length }
                        [Console]::Write((' ' * $before) + $heading + (' ' * $after))
                    }
                    [Console]::Write([string][char]27 + '[97m')
                    [Console]::WriteLine()
                    for ($row = 0; $row -lt 3; $row++) {
                        [Console]::Write(' ' * $gridStart)
                        for ($col = 0; $col -lt 3; $col++) {
                            $index = -1
                            for ($i = 1; $i -lt $items.Count; $i++) {
                                if ($items[$i].Row -eq $row -and $items[$i].Col -eq $col) { $index = $i; break }
                            }
                            if ($index -lt 0) { if ($col -lt 2) { [Console]::Write(' ' * $cellWidth) }; continue }
                            $label = $items[$index].Label
                            $before = [int][Math]::Floor(($cellWidth - $label.Length - 4) / 2)
                            $after = $cellWidth - $before - $label.Length - 4
                            $positions[$index] = @{ X = $gridStart + $col * $cellWidth + $before; GridRow = $row; Offset = [Math]::Min(2, [Math]::Min($before, $after)) }
                            [Console]::Write(' ' * $before)
                            if ($selected -eq $index) {
                                $selectedRow = [Console]::CursorTop
                                $leftArrow = $gridStart + $col * $cellWidth + $before
                                $rightArrow = $leftArrow + $label.Length + 3
                                $maxArrowOffset = [Math]::Min(2, [Math]::Min($before, $after))
                                [Console]::ForegroundColor = [ConsoleColor]::DarkCyan
                                [Console]::Write('> ')
                                [Console]::ForegroundColor = [ConsoleColor]::Cyan
                                [Console]::Write($label)
                                [Console]::ForegroundColor = [ConsoleColor]::DarkCyan
                                [Console]::Write(' <')
                            } else {
                                [Console]::ForegroundColor = [ConsoleColor]::White
                                [Console]::Write('  ' + $label + '  ')
                            }
                            if ($col -lt 2) { [Console]::Write(' ' * $after) }
                        }
                        [Console]::WriteLine()
                    }
                    [Console]::WriteLine()
                    [Console]::WriteLine()
                    $presetLabel = $items[0].Label
                    $presetPadding = [Math]::Max(0, 8 + [int][Math]::Floor(($bannerWidth - ($presetLabel.Length + 4)) / 2))
                    $positions[0] = @{ X = $presetPadding; GridRow = -1; Offset = [Math]::Min(2, $presetPadding) }
                    [Console]::Write(' ' * $presetPadding)
                    if ($selected -eq 0) {
                        $selectedRow = [Console]::CursorTop
                        $leftArrow = $presetPadding
                        $rightArrow = $presetPadding + 3 + $presetLabel.Length
                        [Console]::ForegroundColor = [ConsoleColor]::DarkCyan
                        [Console]::Write('> ')
                        [Console]::ForegroundColor = [ConsoleColor]::Cyan
                        [Console]::Write($presetLabel)
                        [Console]::ForegroundColor = [ConsoleColor]::DarkCyan
                        [Console]::WriteLine(' <')
                    } else {
                        [Console]::ForegroundColor = [ConsoleColor]::White
                        [Console]::WriteLine('  ' + $presetLabel)
                    }
                } else {
                $menuLine = 0
                for ($i = 0; $i -lt $items.Count; $i++) {
                    if ($items[$i].BreakBefore) {
                        [Console]::WriteLine()
                        $menuLine++
                    }
                    if ($items[$i].Detail) {
                        $detailPadding = [Math]::Max(0, 8 + [int][Math]::Floor(($bannerWidth - $items[$i].Detail.Length) / 2))
                        [Console]::ForegroundColor = if ($i -eq $selected) { [ConsoleColor]::Cyan } else { [ConsoleColor]::White }
                        [Console]::WriteLine((' ' * $detailPadding) + $items[$i].Detail)
                        $menuLine++
                    }
                    $padding = [Math]::Max(0, 8 + [int][Math]::Floor(($bannerWidth - ($items[$i].Label.Length + 4)) / 2))
                    $positions[$i] = @{ X = $padding; MenuLine = $menuLine; Offset = [Math]::Min(2, $padding) }
                    [Console]::Write(' ' * $padding)
                    if ($i -eq $selected) {
                        $selectedRow = [Console]::CursorTop
                        $leftArrow = $padding
                        $rightArrow = $padding + 3 + $items[$i].Label.Length
                        [Console]::ForegroundColor = [ConsoleColor]::DarkCyan
                        [Console]::Write('> ')
                        [Console]::ForegroundColor = [ConsoleColor]::Cyan
                        [Console]::Write($items[$i].Label)
                        [Console]::ForegroundColor = [ConsoleColor]::DarkCyan
                        [Console]::WriteLine(' <')
                    } else {
                        [Console]::ForegroundColor = [ConsoleColor]::White
                        [Console]::WriteLine(('  ' + $items[$i].Label))
                    }
                    $menuLine++
                }
                }
                [Console]::ForegroundColor = [ConsoleColor]::White
                [Console]::WriteLine()
                if (-not $grid) {
                    [Console]::WriteLine()
                    [Console]::WriteLine()
                }
                $controlText = 'Arrows:Move    Enter:Select    Esc:Exit'
                $controlPadding = [Math]::Max(0, 8 + [int][Math]::Floor(($bannerWidth - $controlText.Length) / 2))
                [Console]::WriteLine((' ' * $controlPadding) + $controlText)
                [Console]::ForegroundColor = $originalColor
                $returnRow = [Console]::CursorTop
                $returnColumn = [Console]::CursorLeft
                foreach ($index in @($positions.Keys)) {
                    $position = $positions[$index]
                    $position.Y = if ($grid) {
                        if ($position.GridRow -eq -1) { $returnRow - 3 } else { $returnRow - 8 + $position.GridRow }
                    } else {
                        $returnRow - 4 - ($menuLine - $position.MenuLine)
                    }
                }
                $renderWidth = [Console]::WindowWidth
                $renderHeight = [Console]::WindowHeight
                $redraw = $false
                }
                $position = $positions[$selected]
                $selectedRow = $position.Y
                $leftArrow = $position.X
                $rightArrow = $leftArrow + 3 + $items[$selected].Label.Length
                $maxArrowOffset = $position.Offset
                $oldOffset = 0
                $frames = @(0, 1, 2, 1)
                $frame = 0
                $idleClock = [Diagnostics.Stopwatch]::StartNew()
                $nextFrame = 2000
                $requiredRows = if ($grid) { $banner.Count + 12 } else { $banner.Count + 6 + $items.Count + @($items | Where-Object { $_.BreakBefore }).Count + @($items | Where-Object { $_.Detail }).Count }
                if ($title) { $requiredRows += 2 }
                $resized = $false
                while (-not [Console]::KeyAvailable) {
                    [Threading.Thread]::Sleep(10)
                    if ([Console]::KeyAvailable) { break }
                    if ([Console]::WindowWidth -ne $renderWidth -or [Console]::WindowHeight -ne $renderHeight) {
                        $resized = $true
                        break
                    }
                    if ($idleClock.ElapsedMilliseconds -lt $nextFrame) { continue }
                    if ([Console]::WindowWidth -lt ($bannerWidth + 8) -or [Console]::WindowHeight -le $requiredRows) { continue }
                    $nextFrame = $idleClock.ElapsedMilliseconds + 180
                    $frame = ($frame + 1) % $frames.Count
                    $offset = [Math]::Min($frames[$frame], [Math]::Min($leftArrow, $maxArrowOffset))
                    if ($selectedRow -ge [Console]::BufferHeight -or $rightArrow + $offset -ge [Console]::BufferWidth) { continue }
                    [Console]::SetCursorPosition($leftArrow - $oldOffset, $selectedRow)
                    [Console]::Write(' ')
                    [Console]::SetCursorPosition($rightArrow + $oldOffset, $selectedRow)
                    [Console]::Write(' ')
                    [Console]::ForegroundColor = [ConsoleColor]::DarkCyan
                    [Console]::SetCursorPosition($leftArrow - $offset, $selectedRow)
                    [Console]::Write('>')
                    [Console]::SetCursorPosition($rightArrow + $offset, $selectedRow)
                    [Console]::Write('<')
                    [Console]::ForegroundColor = $originalColor
                    [Console]::SetCursorPosition($returnColumn, $returnRow)
                    $oldOffset = $offset
                }
                if ($resized) { $redraw = $true; continue }
                $key = [Console]::ReadKey($true).Key
                $previousSelection = $selected
                switch ($key) {
                    'UpArrow' {
                        if (-not $grid) { $selected = ($selected - 1 + $items.Count) % $items.Count }
                        elseif ($selected -eq 0) { $selected = 5 }
                        elseif ($items[$selected].Row -eq 0) { }
                        else { $selected = @((1..($items.Count - 1)) | Where-Object { $items[$_].Col -eq $items[$selected].Col -and $items[$_].Row -eq ($items[$selected].Row - 1) })[0] }
                    }
                    'DownArrow' {
                        if (-not $grid) { $selected = [Math]::Min($selected + 1, $items.Count - 1) }
                        elseif ($selected -ne 0) {
                            $targetRow = $items[$selected].Row + 1
                            $currentCol = $items[$selected].Col
                            $next = @((1..($items.Count - 1)) | Where-Object { $items[$_].Row -eq $targetRow } | Sort-Object @{ Expression = { [Math]::Abs($items[$_].Col - $currentCol) } }, @{ Expression = { -$items[$_].Col } })
                            if ($next.Count) { $selected = $next[0] } else { $selected = 0 }
                        }
                    }
                    { $_ -in 'LeftArrow', 'RightArrow' } {
                        if ($grid -and $selected -ne 0) {
                            $change = if ($_ -eq 'LeftArrow') { -1 } else { 1 }
                            $targetCol = [Math]::Max(0, [Math]::Min(2, $items[$selected].Col + $change))
                            $candidates = @((1..($items.Count - 1)) | Where-Object { $items[$_].Col -eq $targetCol })
                            $targetRow = [Math]::Min($items[$selected].Row, $candidates.Count - 1)
                            $selected = @($candidates | Where-Object { $items[$_].Row -eq $targetRow })[0]
                        }
                    }
                    'Enter' { return $selected }
                    'Escape' { return -1 }
                }
                if ($selected -ne $previousSelection -or $oldOffset -gt 0) {
                    $repaintItems = if ($selected -eq $previousSelection) { @($selected) } else { @($previousSelection, $selected) }
                    foreach ($index in $repaintItems) {
                        $itemPosition = $positions[$index]
                        $itemLabel = $items[$index].Label
                        $margin = $itemPosition.Offset
                        if ($itemPosition.Y -lt 0 -or $itemPosition.Y -ge [Console]::BufferHeight -or $itemPosition.X + $itemLabel.Length + 4 + $margin -ge [Console]::BufferWidth) {
                            $redraw = $true
                            break
                        }
                        [Console]::SetCursorPosition($itemPosition.X - $margin, $itemPosition.Y)
                        [Console]::ForegroundColor = [ConsoleColor]::White
                        [Console]::Write(' ' * $margin)
                        if ($index -eq $selected) {
                            [Console]::ForegroundColor = [ConsoleColor]::DarkCyan
                            [Console]::Write('> ')
                            [Console]::ForegroundColor = [ConsoleColor]::Cyan
                            [Console]::Write($itemLabel)
                            [Console]::ForegroundColor = [ConsoleColor]::DarkCyan
                            [Console]::Write(' <')
                        } else {
                            [Console]::Write('  ' + $itemLabel + '  ')
                        }
                        [Console]::ForegroundColor = [ConsoleColor]::White
                        [Console]::Write(' ' * $margin)
                    }
                    [Console]::ForegroundColor = $originalColor
                    [Console]::SetCursorPosition($returnColumn, $returnRow)
                }
            }
        } finally {
            [Console]::ForegroundColor = $originalColor
            [Console]::CursorVisible = $originalCursor
        }
    }
    function Save-Preset([string]$path) {
        $relativePath = 'presets/' + [IO.Path]::GetFileName($path)
        $json = [IO.File]::ReadAllText($config)
        $current = $json | ConvertFrom-Json
        $property = $current.PSObject.Properties['preset']
        if ($null -eq $property -or $property.Value -isnot [string]) {
            $current | Add-Member -NotePropertyName 'preset' -NotePropertyValue $relativePath -Force
            $updated = $current | ConvertTo-Json -Depth 100
        } else {
            $pattern = '"preset"\s*:\s*"(?:\\.|[^"\\])*"'
            $matches = [regex]::Matches($json, $pattern)
            if ($matches.Count -ne 1) { throw 'Could not safely update the preset in config.json.' }
            $encoded = ConvertTo-Json -InputObject $relativePath -Compress
            $match = $matches[0]
            $keyEnd = $json.IndexOf(':', $match.Index) + 1
            $valueStart = $keyEnd
            while ([char]::IsWhiteSpace($json[$valueStart])) { $valueStart++ }
            $updated = $json.Substring(0, $valueStart) + $encoded + $json.Substring($match.Index + $match.Length)
        }
        $tempFile = Join-Path (Split-Path -Parent $config) ('.config-' + [Guid]::NewGuid().ToString('N') + '.tmp')
        $backupFile = $tempFile + '.bak'
        try {
            [IO.File]::WriteAllText($tempFile, $updated, (New-Object Text.UTF8Encoding($false)))
            [IO.File]::Replace($tempFile, $config, $backupFile)
        } finally {
            if (Test-Path -LiteralPath $tempFile) { Remove-Item -LiteralPath $tempFile -Force }
            if (Test-Path -LiteralPath $backupFile) { Remove-Item -LiteralPath $backupFile -Force }
        }
    }
    function Select-Preset {
        $presetDir = Join-Path (Get-Location).Path 'presets'
        if (-not (Test-Path -LiteralPath $presetDir -PathType Container)) {
            throw 'No YAML presets found in the presets directory.'
        }
        $presets = @(Get-ChildItem -LiteralPath $presetDir -File | Where-Object { $_.Extension -in '.yaml', '.yml' } | Sort-Object Name)
        if ($presets.Count -eq 0) { throw 'No YAML presets found in the presets directory.' }
        $items = @($presets | ForEach-Object { [pscustomobject]@{ Label = $_.Name } })
        $choice = Select-Item 'Select preset:' $items
        if ($choice -lt 0) { return $null }
        $selected = $presets[$choice].FullName
        Save-Preset $selected
        return $selected
    }
    $config = Join-Path (Get-Location).Path 'config.json'
    if (-not (Test-Path -LiteralPath $config -PathType Leaf)) { throw 'config.json was not found in the project directory.' }
    $settings = Get-Content -LiteralPath $config -Raw | ConvertFrom-Json
    $presetName = $settings.preset
    $preset = $null
    if ($presetName -is [string] -and -not [string]::IsNullOrWhiteSpace($presetName)) {
        $preset = [IO.Path]::GetFullPath((Join-Path (Split-Path -Parent $config) $presetName))
    }
    if (-not $preset -or -not (Test-Path -LiteralPath $preset -PathType Leaf)) {
        $preset = Select-Preset
        if (-not $preset) { [Console]::Clear(); exit 0 }
    }
    $algorithms = @(
        [pscustomobject]@{ Label=''; Value='change-preset' },
        [pscustomobject]@{ Label='Argon2id'; Value='argon2id'; Row=0; Col=0 },
        [pscustomobject]@{ Label='scrypt'; Value='scrypt'; Row=1; Col=0 },
        [pscustomobject]@{ Label='SHA-2'; Value='sha2'; Row=0; Col=1 },
        [pscustomobject]@{ Label='SHA-3'; Value='sha3'; Row=1; Col=1 },
        [pscustomobject]@{ Label='BLAKE3'; Value='blake3'; Row=2; Col=1 },
        [pscustomobject]@{ Label='AES-256-GCM'; Value='aes256gcm'; Row=0; Col=2 },
        [pscustomobject]@{ Label='ChaCha20-Poly1305'; Value='chacha20poly1305'; Row=1; Col=2 }
    )
    while ($true) {
        $algorithms[0].Label = [IO.Path]::GetFileName($preset) + ' selected'
        $choice = Select-Item '' $algorithms 1 $true
        if ($choice -lt 0) { [Console]::Clear(); exit 0 }
        if ($algorithms[$choice].Value -ne 'change-preset') { break }
        $selectedPreset = Select-Preset
        if ($selectedPreset) { $preset = $selectedPreset }
    }
    [Console]::Clear()
    [Console]::WriteLine('Preparing build environment...')
    $environment = @(Receive-Job -Job $buildJob -Wait -ErrorAction Stop)
    foreach ($line in $environment) {
        $equals = $line.IndexOf('=')
        if ($equals -gt 0) { [Environment]::SetEnvironmentVariable($line.Substring(0, $equals), $line.Substring($equals + 1), 'Process') }
    }
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) { throw 'Cargo was not found. Install Rust and Cargo.' }
    & cargo run --release -- --algorithm $algorithms[$choice].Value --config $config --preset $preset
    $code = $LASTEXITCODE
    if ($code -ne 0) {
        [Console]::Error.WriteLine('Benchmark failed; check the message above.')
    } else {
        [Console]::WriteLine('Press any key to close...')
        [void][Console]::ReadKey($true)
    }
    exit $code
} catch {
    [Console]::Error.WriteLine("Error: $($_.Exception.Message)")
    exit 1
} finally {
    if ($buildJob) { $buildJob | Remove-Job -Force -ErrorAction SilentlyContinue }
}

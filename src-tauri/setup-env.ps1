$installDir = $PSScriptRoot
$currentPath = [Environment]::GetEnvironmentVariable("Path", "User")

if ($currentPath -like "*$installDir*") {
    Write-Host "✅ Path already configured: $installDir"
    exit
}

$newPath = "$currentPath;$installDir"
[Environment]::SetEnvironmentVariable("Path", $newPath, "User")
Write-Host "✅ Successfully added to PATH: $installDir"
Write-Host "Please restart your terminal to use 'da' command."
Pause

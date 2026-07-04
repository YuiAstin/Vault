$token = ("protocol=https`nhost=github.com`n" | git credential fill 2>$null | Select-String "password=").ToString().Replace("password=","")
$headers = @{ Authorization = "Bearer $token"; Accept = "application/vnd.github+json" }

# Get the existing release
$release = Invoke-RestMethod -Uri "https://api.github.com/repos/YuiAstin/Vault/releases/tags/v0.2.0" -Headers $headers
Write-Output "Release: $($release.id)"

# Delete old asset if exists
foreach ($asset in $release.assets) {
    if ($asset.name -eq "vault.exe") {
        Write-Output "Deleting old asset: $($asset.id)"
        Invoke-RestMethod -Uri "https://api.github.com/repos/YuiAstin/Vault/releases/assets/$($asset.id)" -Method DELETE -Headers $headers
    }
}

# Upload fresh exe
$uploadUrl = $release.upload_url -replace '\{.*\}', ''
$exePath = "F:\Discord bot\vault\app\src-tauri\target\release\vault.exe"
$uploadHeaders = @{ Authorization = "Bearer $token"; Accept = "application/vnd.github+json"; "Content-Type" = "application/octet-stream" }

Write-Output "Uploading vault.exe..."
$result = Invoke-RestMethod -Uri "$uploadUrl`?name=vault.exe&label=Vault%20(portable%20exe%2C%20Windows%20x64)" -Method POST -Headers $uploadHeaders -InFile $exePath
Write-Output "Uploaded: $($result.name) ($($result.size) bytes)"

# Also upload the installer
$installerPath = "F:\Discord bot\vault\app\src-tauri\target\release\bundle\nsis\vault_0.1.0_x64-setup.exe"
Write-Output "Uploading installer..."
$result2 = Invoke-RestMethod -Uri "$uploadUrl`?name=vault_0.1.0_x64-setup.exe&label=Vault%20Installer%20(Windows%20x64)" -Method POST -Headers $uploadHeaders -InFile $installerPath
Write-Output "Uploaded: $($result2.name) ($($result2.size) bytes)"

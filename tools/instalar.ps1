# Instalador de Felix Felicis para Windows. No necesita Rust: baja los
# binarios precompilados del release, verifica su suma SHA-256 y deja
# universo.exe y universo-mcp.exe en %LOCALAPPDATA%\Programs\felix-felicis\bin
# (o en FELIX_BIN_DIR), y agrega esa carpeta al PATH del usuario.
#
# El workflow de release reemplaza __REPO__ y __VERSION__ y publica este
# archivo (y su .sha256) junto a los binarios; el de la rama no sirve tal cual.
#
# Variables: FELIX_BIN_DIR (carpeta de los binarios), FELIX_VERSION (otro
# tag), FELIX_BASE_URL (otra fuente: URL o carpeta local) y FELIX_NO_PATH=1
# (no tocar el PATH). FELIX_NO_VERIFICAR_VERSION=1 es sólo para las pruebas.
$ErrorActionPreference = 'Stop'

function Falla([string]$mensaje) {
    [Console]::Error.WriteLine("felix: $mensaje")
    exit 1
}

$repo = '__REPO__'
$version = if ($env:FELIX_VERSION) { $env:FELIX_VERSION } else { '__VERSION__' }
$destino = if ($env:FELIX_BIN_DIR) { $env:FELIX_BIN_DIR } else {
    Join-Path $env:LOCALAPPDATA 'Programs\felix-felicis\bin'
}
if (("$repo$version" -match '__') -and -not $env:FELIX_BASE_URL) {
    Falla 'este instalador no fue preparado por el release; descárgalo desde la página del release'
}
$base = if ($env:FELIX_BASE_URL) { $env:FELIX_BASE_URL } else {
    "https://github.com/$repo/releases/download/$version"
}
# Windows ARM64 corre el binario x86_64 emulado.
if (-not [Environment]::Is64BitOperatingSystem) {
    Falla 'no hay binario para Windows de 32 bits; las opciones están en la guía de instalación'
}
$target = 'x86_64-pc-windows-msvc'
$archivo = "felix-felicis-$version-$target.zip"
$binarios = @('universo.exe', 'universo-mcp.exe')
$temporal = Join-Path ([IO.Path]::GetTempPath()) ("felix-" + [Guid]::NewGuid())
New-Item -ItemType Directory -Path $temporal | Out-Null

function Descargar([string]$nombre) {
    $destinoArchivo = Join-Path $temporal $nombre
    if ($base -match '^https?://') {
        Invoke-WebRequest -UseBasicParsing -Uri "$base/$nombre" -OutFile $destinoArchivo
    } else {
        Copy-Item -LiteralPath (Join-Path $base $nombre) -Destination $destinoArchivo
    }
    return $destinoArchivo
}

$error_instalacion = $null
try {
    Write-Host "felix: descargando $archivo"
    $zip = Descargar $archivo
    $suma = Descargar "$archivo.sha256"
    $esperado = ((Get-Content -LiteralPath $suma -Raw).Trim() -split '\s+')[0].ToLowerInvariant()
    $real = (Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($esperado -ne $real) { throw 'la suma SHA-256 no coincide; no se instaló nada' }

    Expand-Archive -LiteralPath $zip -DestinationPath $temporal -Force
    $origen = Join-Path $temporal "felix-felicis-$version-$target"
    foreach ($binario in $binarios) {
        if (-not (Test-Path -LiteralPath (Join-Path $origen $binario))) {
            throw "el archivo no trae $binario; no se instaló nada"
        }
    }
    New-Item -ItemType Directory -Path $destino -Force | Out-Null
    # Un .exe en uso (por ejemplo, el MCP abierto por Claude) no se puede
    # sobrescribir, pero sí renombrar: el anterior pasa a .old y, si algo
    # falla, se restaura.
    $movidos = @()
    try {
        foreach ($binario in $binarios) {
            $final = Join-Path $destino $binario
            if (Test-Path -LiteralPath $final) {
                Move-Item -LiteralPath $final -Destination "$final.old" -Force
                $movidos += $final
            }
            Copy-Item -LiteralPath (Join-Path $origen $binario) -Destination $final
        }
    } catch {
        foreach ($final in $movidos) {
            Remove-Item -LiteralPath $final -Force -ErrorAction SilentlyContinue
            Move-Item -LiteralPath "$final.old" -Destination $final -Force -ErrorAction SilentlyContinue
        }
        throw
    }
    foreach ($final in $movidos) { Remove-Item -LiteralPath "$final.old" -Force -ErrorAction SilentlyContinue }
} catch {
    $error_instalacion = "$_"
} finally {
    Remove-Item -LiteralPath $temporal -Recurse -Force -ErrorAction SilentlyContinue
}
if ($error_instalacion) { Falla $error_instalacion }

Write-Host "felix: instalado en $destino"
if (-not $env:FELIX_NO_VERIFICAR_VERSION) { & (Join-Path $destino 'universo.exe') --version }

# PATH del usuario, leído y escrito en el registro sin expandir: así las
# entradas con %VARIABLES% siguen siendo variables.
if ($env:FELIX_NO_PATH) {
    Write-Host "felix: si $destino no está en tu PATH, agrégalo para usar universo desde cualquier carpeta"
} else {
    $clave = Get-Item -LiteralPath 'HKCU:\Environment'
    $pathUsuario = $clave.GetValue('Path', '', 'DoNotExpandEnvironmentNames')
    $normalizar = { param($p) [Environment]::ExpandEnvironmentVariables($p).TrimEnd('\').ToLowerInvariant() }
    $partes = @($pathUsuario -split ';' | Where-Object { $_ })
    $ya = $partes | Where-Object { (& $normalizar $_) -eq (& $normalizar $destino) }
    if (-not $ya) {
        Set-ItemProperty -LiteralPath 'HKCU:\Environment' -Name 'Path' -Type ExpandString -Value (($partes + $destino) -join ';')
        Write-Host "felix: agregué $destino al PATH de tu usuario; abre una terminal nueva para usarlo"
    }
}
Write-Host 'felix: para empezar:'
Write-Host '  universo init mi-criterio --git; cd mi-criterio; universo conectar'

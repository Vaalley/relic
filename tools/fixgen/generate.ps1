# Relic Test Fixture Generator
#
# Generates a synthetic test library for performance testing.
# Must be executed with PowerShell 7 (pwsh).

param(
    [Parameter(Mandatory = $true, HelpMessage = "The root directory where the library will be generated.")]
    [string]$Root,

    [Parameter(HelpMessage = "The list of systems to generate.")]
    [string[]]$Systems = @('nes', 'snes', 'gb', 'gba', 'megadrive', 'psx'),

    [Parameter(HelpMessage = "The number of game files to generate per system.")]
    [int]$PerSystem = 100,

    [Parameter(HelpMessage = "If set, generates ES-DE style cover media placeholders.")]
    [switch]$WithMedia,

    [Parameter(HelpMessage = "The seed to initialize the random number generator for reproducibility.")]
    [int]$Seed = 42,

    [Parameter(HelpMessage = "Overwrite existing files if set. Re-running without -Force is idempotent.")]
    [switch]$Force
)

# Enable strict mode and fail on errors
Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

# Establish absolute path for Root
$absoluteRoot = [System.IO.Path]::GetFullPath($Root)

# Seed the random generator
$rand = [System.Random]::new($Seed)

# Word lists for game name generation
$adjectives = @(
    "Super", "Mega", "Final", "Golden", "Sonic", "Double", "Ultra", "Chrono", 
    "Wild", "Secret", "Space", "Star", "Pocket", "Retro", "Neo", "Grand", 
    "Epic", "Ultimate", "Cyber", "Atomic", "Shadow", "Dragon", "Metal", "Fantasy", 
    "Dark", "Magic", "Virtual", "Classic", "Extreme", "Mighty", "Perfect", "Silent", 
    "Lost", "Ancient", "Primal", "Galaxy", "Cosmic", "Radical", "Wonder", "Monster",
    "Crazy", "Tiny", "Iron", "Steel", "Brave", "Phantasy", "Championship", "Shin"
)
$nouns = @(
    "Quest", "Trigger", "Fantasy", "Combat", "Fighter", "Force", "Adventure", "Hero", 
    "Rider", "Ranger", "Championship", "World", "Galaxy", "Castle", "Monsters", "Legends", 
    "Warriors", "Crusade", "Empire", "Odyssey", "Rumble", "Duel", "Runner", "Striker", 
    "Zero", "Chronicles", "Command", "Vanguard", "Destiny", "Saga", "Legacy", "Origin", 
    "Eclipse", "Havoc", "Panic", "Blaze", "Storm", "Soldier", "Island", "Kingdom",
    "Basement", "Metroid", "Tomb", "Scroll", "Rings", "Stars", "Revenge", "Mission"
)
$suffixes = @("II", "III", "IV", "V", "64", "3D", "2", "3", "Advance", "Returns", "Unleashed", "DX", "Turbo", "Classic")
$prepositions = @("of", "for", "against", "in", "from", "to")
$regions = @("(USA)", "(Europe)", "(Japan)", "(World)", "(France)", "(Germany)")
$revisions = @("", " (Rev 1)", " (Rev A)", " (v1.1)", " (v1.2)", " (Rev B)")

# Unique name generator utilizing the seeded System.Random
function Get-RandomName {
    param(
        [System.Random]$Random
    )
    $struct = $Random.Next(0, 100)
    $name = ""
    if ($struct -lt 30) {
        $adj = $adjectives[$Random.Next(0, $adjectives.Length)]
        $noun = $nouns[$Random.Next(0, $nouns.Length)]
        $name = "$adj $noun"
    } elseif ($struct -lt 60) {
        $adj = $adjectives[$Random.Next(0, $adjectives.Length)]
        $noun = $nouns[$Random.Next(0, $nouns.Length)]
        $sfx = $suffixes[$Random.Next(0, $suffixes.Length)]
        $name = "$adj $noun $sfx"
    } elseif ($struct -lt 80) {
        $adj1 = $adjectives[$Random.Next(0, $adjectives.Length)]
        $adj2 = $adjectives[$Random.Next(0, $adjectives.Length)]
        while ($adj1 -eq $adj2) {
            $adj2 = $adjectives[$Random.Next(0, $adjectives.Length)]
        }
        $noun = $nouns[$Random.Next(0, $nouns.Length)]
        $name = "$adj1 $adj2 $noun"
    } else {
        $noun1 = $nouns[$Random.Next(0, $nouns.Length)]
        $prep = $prepositions[$Random.Next(0, $prepositions.Length)]
        $noun2 = $nouns[$Random.Next(0, $nouns.Length)]
        while ($noun1 -eq $noun2) {
            $noun2 = $nouns[$Random.Next(0, $nouns.Length)]
        }
        $name = "The $noun1 $prep $noun2"
    }
    
    $reg = $regions[$Random.Next(0, $regions.Length)]
    $rev = ""
    if ($Random.Next(0, 100) -lt 25) {
        $rev = $revisions[$Random.Next(1, $revisions.Length)]
    }
    
    return "$name $reg$rev"
}

# Read platform systems directory relative to script root
$systemsDir = [System.IO.Path]::GetFullPath([System.IO.Path]::Combine($PSScriptRoot, "..\..\core\data\systems"))

function Get-SystemExtension {
    param(
        [string]$SystemSlug
    )
    $tomlPath = Join-Path $systemsDir "$SystemSlug.toml"
    
    if (-not (Test-Path $tomlPath)) {
        throw "System configuration TOML not found: $tomlPath"
    }
    
    $content = Get-Content $tomlPath -Raw
    if ($content -match 'extensions\s*=\s*\[([^\]]+)\]') {
        $extsStr = $Matches[1]
        [array]$exts = $extsStr.Split(',') | ForEach-Object { $_.Trim().Trim('"').Trim("'") }
        # Exclude standard archive files to pick first raw file extension
        [array]$nonArchive = $exts | Where-Object { $_ -ne "zip" -and $_ -ne "7z" -and $_ -ne "rar" -and $_ -ne "tar" }
        if ($nonArchive.Length -gt 0) {
            return $nonArchive[0]
        }
    }
    throw "Failed to extract a valid non-archive extension from TOML: $tomlPath"
}

# Main execution
Write-Host "Relic Test Fixture Generator"
Write-Host "============================"
Write-Host "Root path:       $absoluteRoot"
Write-Host "Systems to gen:  $($Systems -join ', ')"
Write-Host "Games per sys:   $PerSystem"
Write-Host "Media enabled:   $WithMedia"
Write-Host "Random Seed:     $Seed"
Write-Host "Force Overwrite: $Force"
Write-Host ""

# Ensure base directory exists
if (-not (Test-Path $absoluteRoot)) {
    New-Item -ItemType Directory -Path $absoluteRoot -Force | Out-Null
}

$totalRomFiles = 0
$totalMediaFiles = 0
$totalFilesCreated = 0

# Sort systems for deterministic randomness across runs
$sortedSystems = $Systems | Sort-Object

$placeholderBytes = [System.Text.Encoding]::ASCII.GetBytes("relic test fixture")

foreach ($system in $sortedSystems) {
    # Retrieve extension
    $extension = Get-SystemExtension -SystemSlug $system
    
    # Setup directories
    $systemDir = Join-Path $absoluteRoot $system
    New-Item -ItemType Directory -Path $systemDir -Force | Out-Null
    
    $coversDir = Join-Path $systemDir "media\covers"
    if ($WithMedia) {
        New-Item -ItemType Directory -Path $coversDir -Force | Out-Null
    }
    
    $generatedNames = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase)
    
    Write-Host "Processing system '$system' (extension: .$extension)..."
    
    for ($i = 0; $i -lt $PerSystem; $i++) {
        # Loop until unique name is found
        $romStem = Get-RandomName -Random $rand
        $attempts = 0
        while ($generatedNames.Contains($romStem) -and $attempts -lt 1000) {
            $romStem = (Get-RandomName -Random $rand) + " (" + $rand.Next(1, 10000) + ")"
            $attempts++
        }
        $generatedNames.Add($romStem) | Out-Null
        
        # Create ROM path
        $romFilename = "$romStem.$extension"
        $romPath = Join-Path $systemDir $romFilename
        
        if (-not (Test-Path $romPath) -or $Force) {
            [System.IO.File]::WriteAllBytes($romPath, $placeholderBytes)
            $totalFilesCreated++
        }
        $totalRomFiles++
        
        # Create Media cover path if requested
        if ($WithMedia) {
            $mediaFilename = "$romStem.png"
            $mediaPath = Join-Path $coversDir $mediaFilename
            if (-not (Test-Path $mediaPath) -or $Force) {
                [System.IO.File]::WriteAllBytes($mediaPath, $placeholderBytes)
                $totalFilesCreated++
            }
            $totalMediaFiles++
        }
    }
}

$totalProcessed = $totalRomFiles + $totalMediaFiles
Write-Host ""
Write-Host "Summary:"
Write-Host "--------"
Write-Host "Total Systems Processed: $($sortedSystems.Length)"
Write-Host "Total ROM placeholders:  $totalRomFiles"
Write-Host "Total Cover placeholders:$totalMediaFiles"
Write-Host "Total files on disk:     $totalProcessed"
Write-Host "New/Overwritten files:   $totalFilesCreated"
Write-Host "Done!"

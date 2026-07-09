param(
    [string]$ProjectDir = $(if ($env:CC_UAX_STACKOBOT_PROJECT_DIR) { $env:CC_UAX_STACKOBOT_PROJECT_DIR } else { 'D:/WorkDir/StackOBot' }),
    [string]$ContentDir = $(if ($env:CC_UAX_CONTENT_DIR) { $env:CC_UAX_CONTENT_DIR } else { 'D:/WorkDir/StackOBot/Content' }),
    [string]$Exe = $env:CC_UAX_EXE,
    [string]$Output = $(if ($env:CC_UAX_STACKOBOT_REPORT) { $env:CC_UAX_STACKOBOT_REPORT } else { '' }),
    [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = Resolve-Path (Join-Path $ScriptDir '..')
if (-not $Exe) {
    $Exe = Join-Path $RepoRoot 'target/release/cc-uax.exe'
}
if (-not $Output) {
    $Output = Join-Path $RepoRoot 'target/stackobot-gameplay-report.md'
}

if (-not $SkipBuild -and -not (Test-Path $Exe)) {
    Push-Location $RepoRoot
    try {
        cargo build --release --locked
    } finally {
        Pop-Location
    }
}

if (-not (Test-Path $Exe)) {
    throw "cc-uax executable not found: $Exe"
}
if (-not (Test-Path $ProjectDir)) {
    throw "StackOBot project directory not found: $ProjectDir"
}
if (-not (Test-Path $ContentDir)) {
    throw "StackOBot content directory not found: $ContentDir"
}

$ProjectDir = (Resolve-Path $ProjectDir).Path
$ContentDir = (Resolve-Path $ContentDir).Path
$ConfigPath = Join-Path $ProjectDir 'Config/DefaultEngine.ini'
if (-not (Test-Path $ConfigPath)) {
    throw "DefaultEngine.ini not found: $ConfigPath"
}

function Invoke-CcUaxJson {
    param([string[]]$CliArgs)

    $output = & $Exe @CliArgs 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "cc-uax failed ($LASTEXITCODE): $($CliArgs -join ' ')`n$($output -join "`n")"
    }
    $text = ($output | Where-Object { $_ -match '^\s*\{' } | Select-Object -Last 1)
    if (-not $text) {
        throw "cc-uax did not emit JSON: $($CliArgs -join ' ')`n$($output -join "`n")"
    }
    $text | ConvertFrom-Json
}

function Get-ConfigValue {
    param([string]$Name)

    $line = Get-Content -Path $ConfigPath |
        Where-Object { $_ -match "^\s*$([regex]::Escape($Name))\s*=\s*(.+?)\s*$" } |
        Select-Object -First 1
    if ($line -and $line -match "^\s*$([regex]::Escape($Name))\s*=\s*(.+?)\s*$") {
        return $Matches[1]
    }
    ''
}

function Convert-ToGamePath {
    param([string]$FilePath)

    $relative = [System.IO.Path]::GetRelativePath($ContentDir, $FilePath)
    $withoutExtension = $relative -replace '\.(uasset|umap)$', ''
    '/Game/' + ($withoutExtension -replace '\\', '/')
}

function Get-NodeLabel {
    param($Node)

    if (-not $Node) {
        return '<missing>'
    }
    if ($Node.member) {
        return "$($Node.member) [$($Node.name)]"
    }
    if ($Node.name) {
        $className = [string]$Node.class
        $shortClass = if ($className -match '\.([^\.]+)$') { $Matches[1] } else { $className }
        return "$($Node.name) <$shortClass>"
    }
    '<unnamed>'
}

function New-LogicModel {
    param(
        [string]$Id,
        [string]$Label,
        [string]$Relative
    )

    $file = Join-Path $ContentDir $Relative
    if (-not (Test-Path $file)) {
        throw "StackOBot asset not found for report: $file"
    }
    $file = (Resolve-Path $file).Path
    Write-Host "Parsing logic: $Label"
    $json = Invoke-CcUaxJson -CliArgs @('-S', 'logic', '--compact', $file)
    $exports = @($json.exports)
    $byIndex = @{}
    foreach ($export in $exports) {
        $byIndex[[int]$export.index] = $export
    }

    $edges = @()
    foreach ($export in $exports) {
        foreach ($pin in @($export.pins)) {
            if ($pin.category -ne 'exec' -or $pin.direction -ne 'output' -or -not $pin.linked_to) {
                continue
            }
            foreach ($link in @($pin.linked_to)) {
                $target = $byIndex[[int]$link.node_index]
                $from = Get-NodeLabel $export
                $to = Get-NodeLabel $target
                $edges += [pscustomobject]@{
                    Asset = $Label
                    From = $from
                    Pin = [string]$pin.name
                    To = $to
                    Text = "$($Label): $from --$($pin.name)--> $to"
                }
            }
        }
    }

    [pscustomobject]@{
        Id = $Id
        Label = $Label
        File = $file
        GamePath = Convert-ToGamePath $file
        Diagnostics = @($json.diagnostics).Count
        Exports = $exports
        Functions = @($exports | Where-Object { $_.class -eq '/Script/CoreUObject.Function' } | ForEach-Object { $_.name } | Sort-Object -Unique)
        Members = @($exports | Where-Object { $_.member } | ForEach-Object { $_.member } | Where-Object { $_ -and $_ -ne 'None' } | Sort-Object -Unique)
        Events = @($exports | Where-Object { $_.name -match 'Receive|InpAct|BndEvt|Event|FunctionEntry|EnhancedInputAction' } | ForEach-Object { Get-NodeLabel $_ } | Sort-Object -Unique)
        Edges = $edges
    }
}

function New-RefsModel {
    param(
        [string]$Label,
        [string]$Relative
    )

    $file = Join-Path $ContentDir $Relative
    if (-not (Test-Path $file)) {
        throw "StackOBot asset not found for refs: $file"
    }
    $file = (Resolve-Path $file).Path
    Write-Host "Parsing refs: $Label"
    $json = Invoke-CcUaxJson -CliArgs @(
        '-S', 'refs',
        '--scan-dir', $ContentDir,
        '--no-cache',
        '--compact',
        $file
    )
    [pscustomobject]@{
        Label = $Label
        File = $file
        GamePath = $(if ($json.references.self) { $json.references.self } else { Convert-ToGamePath $file })
        Diagnostics = @($json.diagnostics).Count
        Assets = @($json.references.assets)
        ReferencedBy = @($json.references.referenced_by)
        Scripts = @($json.references.scripts)
    }
}

function Select-Edges {
    param(
        [object[]]$Models,
        [string[]]$Patterns,
        [int]$Max = 10
    )

    $results = @()
    foreach ($model in $Models) {
        foreach ($edge in @($model.Edges)) {
            foreach ($pattern in $Patterns) {
                if ($edge.Text -match $pattern) {
                    $results += [string]$edge.Text
                    break
                }
            }
        }
    }
    @($results | Sort-Object -Unique | Select-Object -First $Max)
}

function Select-Names {
    param(
        [object[]]$Items,
        [string[]]$Patterns,
        [int]$Max = 20
    )

    $results = @()
    foreach ($item in $Items) {
        foreach ($pattern in $Patterns) {
            if ([string]$item -match $pattern) {
                $results += [string]$item
                break
            }
        }
    }
    @($results | Sort-Object -Unique | Select-Object -First $Max)
}

function Format-CodeList {
    param([object[]]$Items)

    $list = @($Items | Where-Object { $_ } | Sort-Object -Unique)
    if ($list.Count -eq 0) {
        return '无'
    }
    ($list | ForEach-Object { "``$_``" }) -join '、'
}

function Add-Evidence {
    param(
        [System.Collections.Generic.List[string]]$Lines,
        [string]$Claim,
        [object[]]$Evidence,
        [switch]$Required
    )

    $items = @($Evidence | Where-Object { $_ })
    if ($Required -and $items.Count -eq 0) {
        throw "required gameplay evidence missing: $Claim"
    }
    $Lines.Add("- $Claim")
    if ($items.Count -eq 0) {
        $Lines.Add("  - 解析证据：未在当前解析输出中命中。")
    } else {
        foreach ($item in $items) {
            $Lines.Add("  - 解析证据：``$item``")
        }
    }
}

function Add-Section {
    param(
        [System.Collections.Generic.List[string]]$Lines,
        [string]$Title
    )
    $Lines.Add('')
    $Lines.Add("## $Title")
}

$assetFiles = @(Get-ChildItem -Path $ContentDir -Recurse -File -Include '*.uasset', '*.umap')

$logic = @{}
foreach ($spec in @(
    @('MainMenuUi', 'UI_MainMenu', 'StackOBot/UI/MainMenu/UI_MainMenu.uasset'),
    @('GameInstance', 'GI_StackOBot', 'StackOBot/Blueprints/Framework/GI_StackOBot.uasset'),
    @('GameMode', 'GM_InGame', 'StackOBot/Blueprints/Framework/GM_InGame.uasset'),
    @('PlayerController', 'BP_PC_Stack', 'StackOBot/Blueprints/Character/BP_PC_Stack.uasset'),
    @('PlayerPawn', 'BP_Bot', 'StackOBot/Blueprints/Character/BP_Bot.uasset'),
    @('Coin', 'BP_Coin', 'StackOBot/Blueprints/GameElements/BP_Coin.uasset'),
    @('Cog', 'BP_Cog', 'StackOBot/Blueprints/GameElements/BP_Cog.uasset'),
    @('Portal', 'BP_Portal', 'StackOBot/Blueprints/GameElements/BP_Portal.uasset'),
    @('Bridge', 'BP_Bridge', 'StackOBot/Blueprints/PhysicsElements/BP_Bridge.uasset'),
    @('BouncePad', 'BP_BouncePad', 'StackOBot/Blueprints/PhysicsElements/BP_BouncePad.uasset'),
    @('Enemy', 'BP_Bug', 'StackOBot/AI/BP_Bug.uasset'),
    @('AiController', 'BP_AIController', 'StackOBot/AI/BP_AIController.uasset'),
    @('GameUi', 'UI_Game', 'StackOBot/UI/Game/UI_Game.uasset')
)) {
    $logic[$spec[0]] = New-LogicModel -Id $spec[0] -Label $spec[1] -Relative $spec[2]
}

$refs = @{}
foreach ($spec in @(
    @('MainMenuMap', 'LVL_MainMenu', 'StackOBot/Maps/LVL_MainMenu.umap'),
    @('GameMap', 'LVL_StackOBot', 'StackOBot/Maps/LVL_StackOBot.umap'),
    @('StateTree', 'STree_Bug', 'StackOBot/AI/STree_Bug.uasset'),
    @('Coin', 'BP_Coin', 'StackOBot/Blueprints/GameElements/BP_Coin.uasset'),
    @('Cog', 'BP_Cog', 'StackOBot/Blueprints/GameElements/BP_Cog.uasset'),
    @('Portal', 'BP_Portal', 'StackOBot/Blueprints/GameElements/BP_Portal.uasset'),
    @('PlayerSave', 'PlayerSaveObject', 'StackOBot/Blueprints/SaveGame/PlayerSaveObject.uasset'),
    @('LevelSave', 'LevelSaveObject', 'StackOBot/Blueprints/SaveGame/LevelSaveObject.uasset'),
    @('CollectableSave', 'CollectableObjectData', 'StackOBot/Blueprints/SaveGame/CollectableObjectData.uasset')
)) {
    $refs[$spec[0]] = New-RefsModel -Label $spec[1] -Relative $spec[2]
}

$logicDiagnostics = ($logic.Values | Measure-Object -Property Diagnostics -Sum).Sum
$refsDiagnostics = ($refs.Values | Measure-Object -Property Diagnostics -Sum).Sum
if (($logicDiagnostics + $refsDiagnostics) -gt 0) {
    throw "report source diagnostics are not clean: logic=$logicDiagnostics refs=$refsDiagnostics"
}

$lines = [System.Collections.Generic.List[string]]::new()
$lines.Add('# StackOBot 蓝图玩法逻辑验收报告')
$lines.Add('')
$lines.Add('> 本报告只使用 `cc-uax -S logic` 的蓝图节点/pin 执行链路和 `cc-uax -S refs` 的引用图生成。每条玩法说明下面都列出解析证据；证据缺失时脚本会失败或显式标注，避免用文件名脑补玩法。')
$lines.Add('')
$lines.Add("- 生成时间：$(Get-Date -Format 'yyyy-MM-dd HH:mm:ss zzz')")
$lines.Add("- 项目目录：``$ProjectDir``")
$lines.Add("- 内容目录：``$ContentDir``")
$lines.Add("- 资源总量：$($assetFiles.Count) 个 ``.uasset/.umap``")
$lines.Add("- 逻辑资产：$($logic.Count) 个；引用资产：$($refs.Count) 个；源 diagnostics：$($logicDiagnostics + $refsDiagnostics)")

Add-Section $lines '入口与关卡流程'
$lines.Add("- ``GameDefaultMap``：``$(Get-ConfigValue 'GameDefaultMap')``")
$lines.Add("- ``GameInstanceClass``：``$(Get-ConfigValue 'GameInstanceClass')``")
$lines.Add("- ``GlobalDefaultGameMode``：``$(Get-ConfigValue 'GlobalDefaultGameMode')``")
Add-Evidence $lines '主菜单 Start 按钮进入游戏关卡，随后菜单 UI 从视口移除。' `
    (Select-Edges @($logic.MainMenuUi) @('Button Pressed.*OpenLevel', 'OpenLevel.*RemoveFromParent')) -Required
Add-Evidence $lines '主菜单 Quit 按钮调用退出接口。' `
    (Select-Edges @($logic.MainMenuUi) @('Button Pressed.*Quit')) -Required
Add-Evidence $lines 'GameInstance 的 OpenLevel 先开加载界面并暂停，延时后真正 OpenLevel。' `
    (Select-Edges @($logic.GameInstance) @('OpenLevel.*ToggleLoadingScreen', 'ToggleLoadingScreen.*SetGamePaused', 'SetGamePaused.*Delay', 'Delay.*OpenLevel')) -Required
Add-Evidence $lines 'GameMode 的 End Game 事件会回调 GameInstance.OpenLevel，传送门完成后会触发这条结束流程。' `
    (Select-Edges @($logic.GameMode, $logic.Portal) @('End Game.*OpenLevel', 'Delay.*End Game', 'Trigger Complete.*SetCustomPrimitiveDataFloat')) -Required

Add-Section $lines '玩家生成、重生与 possession'
Add-Evidence $lines 'GameMode 收到 Spawn 事件后生成玩家 Pawn，并由 PlayerController Possess，随后播放出生动画并广播 PlayerRespawned。' `
    (Select-Edges @($logic.GameMode) @('Spawn .*SpawnActorFromClass', 'SpawnActorFromClass.*Possess', 'Possess.*StartSpawnAnimation', 'StartSpawnAnimation.*PlayerRespawned')) -Required
Add-Evidence $lines 'BeginPlay 会初始化加载界面、查找 SpawnPad，并设置 ActiveSpawnPad 后读取出生 Transform。' `
    (Select-Edges @($logic.GameMode) @('ReceiveBeginPlay.*ExecutionSequence', 'GetAllActorsOfClassWithTag.*ActiveSpawnPad', 'ActiveSpawnPad.*Retrieve Spawn Transform', 'Retrieve Spawn Transform.*K2_SetActorLocationAndRotation')) -Required
Add-Evidence $lines 'PlayerController 还能在 Reset Player / drone 流程中重新生成或切换 Pawn possession。' `
    (Select-Edges @($logic.PlayerController) @('StopMovementImmediately.*SpawnActorFromClass', 'SpawnActorFromClass.*DroneRef', 'DroneRef.*Possess', 'K2_SetActorLocation.*Possess')) -Required

Add-Section $lines '玩家操作与移动能力'
$playerInputs = Select-Names ($logic.PlayerPawn.Functions + $logic.PlayerPawn.Members + $logic.PlayerController.Functions + $logic.PlayerController.Members) @('IA_Move', 'IA_MoveSideScrolling', 'IA_Look', 'IA_Jump', 'IA_Grab', 'IA_Pause', 'IA_CameraToggle', 'IA_DroneSwitch')
$lines.Add("- 从 Enhanced Input 节点解析到的操作输入：$(Format-CodeList $playerInputs)")
Add-Evidence $lines '移动输入最终进入 AddMovementInput；Look 输入进入 yaw/pitch 控制。' `
    (Select-Edges @($logic.PlayerPawn) @('EnhancedInputAction.*Knot', 'Knot.*AddMovementInput', 'AddControllerYawInput.*AddControllerPitchInput')) -Required
Add-Evidence $lines 'Jump 输入按 MovementMode 分支：地面调用 Jump，空中/飞行调用 ToggleJetpack；松开输入 StopJumping 并关闭/切换喷气。' `
    (Select-Edges @($logic.PlayerPawn) @('SwitchEnum.*Jump', 'SwitchEnum.*ToggleJetpack', 'Completed.*StopJumping', 'StopJumping.*ToggleJetpack')) -Required
Add-Evidence $lines '喷气逻辑持续更新推进、特效、音频和燃料显示。' `
    (Select-Edges @($logic.PlayerPawn) @('ReceiveTick.*ExecutionSequence', 'ExecutionSequence.*Update Jetpack', 'Update Jetpack.*RenderShadow', 'LaunchCharacter.*SetVariableFloat', 'SetActive.*IfThenElse', 'JetpackActive.*IfThenElse')) -Required
Add-Evidence $lines '抓取/交互先做 LineTrace，再 Grab_Init / Grab_Update / Grab_Clear 控制 PhysicsHandle、移动目标、音效和 Niagara。' `
    (Select-Edges @($logic.PlayerPawn) @('Grab_Check.*LineTraceSingle', 'Grab_Init.*GrabComponentAtLocationWithRotation', 'Grab_Update.*SetTargetLocationAndRotation', 'Grab_Clear.*ReleaseComponent', 'SpawnSystemAttached.*FX_Grab')) -Required
Add-Evidence $lines '暂停输入进入 GameInstance/接口的 SetPaused；相机状态会切换平面约束和输入映射。' `
    (Select-Edges @($logic.PlayerController) @('Pause.*Knot', 'Knot.*SetPaused', 'SwitchEnum.*SetPlaneConstraintEnabled', 'SetPlaneConstraintEnabled.*AddMappingContext', 'SetPlaneConstraintEnabled.*RemoveMappingContext')) -Required

Add-Section $lines '收集物、目标与机关'
Add-Evidence $lines '金币在 ActorBeginOverlap 后触发收集：更新金币数、关闭碰撞、播放音效和 Niagara，时间线完成后销毁自身。' `
    (Select-Edges @($logic.Coin) @('ReceiveActorBeginOverlap.*IfThenElse', 'IfThenElse.*Trigger Collection', 'Trigger Collection.*UpdateCoins', 'UpdateCoins.*SetCollisionEnabled', 'SetCollisionEnabled.*PlaySoundAtLocation', 'PlaySoundAtLocation.*SpawnSystemAtLocation', 'Timeline_0.*DestroyActor')) -Required
Add-Evidence $lines '齿轮蓝图解析到的是旋转/材质/缩放表现逻辑；当前 BP_Cog 中没有可证明的 overlap 收集 exec 链路，报告不把它脑补成金币同类收集逻辑。' `
    (Select-Edges @($logic.Cog) @('UserConstructionScript.*SetComponentTickEnabled', 'SetComponentTickEnabled.*SetRelativeScale3D', 'RotationRate.*CreateDynamicMaterialInstance', 'SetMaterial.*SetVectorParameterValue')) -Required
Add-Evidence $lines '目标 UI 更新由 GameMode.Update Objective 写入 Objective Tracker 后调用 HUD/UI 的 Update Objective UI。' `
    (Select-Edges @($logic.GameMode) @('Update Objective.*Array_Set', 'Array_Set.*Update Objective UI')) -Required
Add-Evidence $lines '传送门 overlap 完成后播放特效、切换相机、禁用输入、启用 tick，延时后触发 End Game。' `
    (Select-Edges @($logic.Portal) @('ReceiveActorBeginOverlap.*MacroInstance', 'MacroInstance.*SpawnSystemAttached', 'SpawnSystemAttached.*SetViewTargetWithBlend', 'SetViewTargetWithBlend.*DisableInput', 'DisableInput.*SetActorTickEnabled', 'SetActorTickEnabled.*Delay', 'Delay.*End Game')) -Required
Add-Evidence $lines '桥和弹跳板是机关类蓝图：桥使用组件/连接件逻辑，弹跳板 overlap 后进入跳板音效/Pad 特效链路。' `
    (Select-Edges @($logic.Bridge, $logic.BouncePad) @('Bridge', 'Bounce', 'ComponentBeginOverlap', 'PlaySoundAtLocation', 'LaunchCharacter', 'FX_Pad') 12)

Add-Section $lines '敌人 AI 与战斗'
Add-Evidence $lines 'AIController 在 BeginPlay 绑定 OnPossessedPawnChanged，并在 possessed pawn 有效时 StartLogic；这证明敌人 AI 由控制器启动逻辑。' `
    (Select-Edges @($logic.AiController) @('ReceiveBeginPlay.*OnPossessedPawnChanged', 'OnPossessedPawnChanged.*MacroInstance', 'MacroInstance.*StartLogic')) -Required
$stateTreeAssets = @($refs.StateTree.Assets | Where-Object { $_ -match 'StateTree_Elements|EQS|BP_AIController|BP_Bug' } | Sort-Object)
if ($stateTreeAssets.Count -eq 0) {
    throw 'required StateTree refs missing'
}
$lines.Add("- ``STree_Bug`` 的引用图证明敌人行为由 StateTree/EQS/task/condition 资产组成：$(Format-CodeList $stateTreeAssets)")
Add-Evidence $lines 'BP_Bug 的 overlap 分支会对玩家发送 RecieveDamage；另一条死亡链会播放音效、设置 Dying、禁用移动、播放 montage，并在完成后销毁。' `
    (Select-Edges @($logic.Enemy) @('ComponentBeginOverlap.*IfThenElse', 'Knot.*RecieveDamage', 'MacroInstance.*PlaySoundAtLocation', 'PlaySoundAtLocation.*Dying', 'Dying.*DisableMovement', 'DisableMovement.*PlayMontage', 'PlayMontage.*DestroyActor')) -Required
Add-Evidence $lines 'BP_Bug 还解析到 Delay -> Jump -> Velocity 的跳跃/攻击动作链。' `
    (Select-Edges @($logic.Enemy) @('Delay.*Jump', 'Jump.*Velocity')) -Required

Add-Section $lines 'UI 与存档'
Add-Evidence $lines 'UI_Game 初始化时 cast 到 GameInstance 并绑定 OnCoinsUpdated；金币变化后直接 SetText 更新数量。' `
    (Select-Edges @($logic.GameUi) @('OnInitialized.*DynamicCast', 'DynamicCast.*OnCoinsUpdated', 'OnCoinsUpdated.*SetText')) -Required
Add-Evidence $lines 'UI_Game 的目标列表在 Objective Check 中复用/创建 UI_Objective，AddChildToGrid 后调用 Update Objective 并 Show UI。' `
    (Select-Edges @($logic.GameUi) @('Objective Check.*MacroInstance', 'MacroInstance.*Update Objective', 'MacroInstance.*CreateWidget', 'CreateWidget.*Array_Set', 'Array_Set.*AddChildToGrid', 'AddChildToGrid.*Update Objective', 'Update Objective.*Show UI')) -Required
Add-Evidence $lines 'GameInstance 初始化存档：检查 slot，存在则 LoadGameFromSlot，不存在则 CreateSaveGameObject；保存走 SaveGameToSlot。' `
    (Select-Edges @($logic.GameInstance) @('ReceiveInit.*InitPlayerSaveData', 'InitPlayerSaveData.*DoesSaveGameExist', 'IfThenElse.*LoadGameFromSlot', 'IfThenElse.*CreateSaveGameObject', 'SaveGame.*SaveGameToSlots', 'SaveGameToSlots.*SaveGameToSlot')) -Required
Add-Evidence $lines '金币数保存在 GameInstance.Orbs，并通过 OnCoinsUpdated 委托驱动 UI。' `
    (Select-Edges @($logic.GameInstance) @('UpdateCoins.*Orbs', 'Orbs.*OnCoinsUpdated')) -Required
$saveRefs = @(
    "PlayerSaveObject referenced_by: $((@($refs.PlayerSave.ReferencedBy) | Sort-Object) -join ', ')",
    "LevelSaveObject referenced_by: $((@($refs.LevelSave.ReferencedBy) | Sort-Object) -join ', ')",
    "CollectableObjectData referenced_by: $((@($refs.CollectableSave.ReferencedBy) | Sort-Object) -join ', ')"
)
foreach ($saveRef in $saveRefs) {
    $lines.Add("- 解析证据：``$saveRef``")
}

Add-Section $lines '玩法流程总结'
$lines.Add('1. 进入游戏先加载 `LVL_MainMenu`；主菜单 Start 按钮调用 `OpenLevel` 进入主关卡，Quit 按钮调用退出接口。')
$lines.Add('2. 进入主关卡后，`GM_InGame.ReceiveBeginPlay` 初始化加载屏、出生点和目标；`Spawn` 事件生成 `BP_Bot` 并由 `BP_PC_Stack` possess。')
$lines.Add('3. 玩家通过 Enhanced Input 移动、看向、跳跃、抓取、暂停；跳跃在地面是 `Jump`，空中/飞行状态会进入 `ToggleJetpack` 喷气链路。')
$lines.Add('4. 收集物方面，`BP_Coin` overlap 明确更新金币、关闭碰撞、播放反馈并销毁；`BP_Cog` 当前可证明的是旋转/材质表现逻辑，没有在该蓝图内解析到收集 exec 链路。')
$lines.Add('5. 关卡推进由目标更新、机关、传送门和 GameMode/GameInstance 协作完成；传送门完成后触发 End Game，再由 GameInstance 打开关卡。')
$lines.Add('6. 敌人由 `BP_AIController` 启动逻辑，`STree_Bug` 引用 StateTree task/condition/EQS；`BP_Bug` overlap 可造成玩家伤害，也有受击死亡 montage 和销毁链。')
$lines.Add('7. UI 绑定 GameInstance 的金币委托并更新文本，同时维护目标 UI；GameInstance 负责 slot 检查、加载、创建和保存 SaveGame。')

Add-Section $lines '解析覆盖检查'
$lines.Add("- 所有报告源 ``diagnostics`` 为 0。")
$lines.Add("- 关键逻辑资产：$(Format-CodeList ($logic.Values | ForEach-Object { $_.GamePath }))")
$lines.Add("- 关键引用资产：$(Format-CodeList ($refs.Values | ForEach-Object { $_.GamePath }))")
$lines.Add("- 说明：如果后续解析器遗漏 Blueprint pins 或 member distillation，本脚本的 Required evidence 会失败，报告不会静默生成。")

$requiredSections = @(
    '入口与关卡流程',
    '玩家生成、重生与 possession',
    '玩家操作与移动能力',
    '收集物、目标与机关',
    '敌人 AI 与战斗',
    'UI 与存档',
    '玩法流程总结',
    '解析覆盖检查'
)
$reportText = $lines -join "`n"
foreach ($section in $requiredSections) {
    if ($reportText -notmatch "## $([regex]::Escape($section))") {
        throw "gameplay report missing required section: $section"
    }
}

$outputDir = Split-Path -Parent $Output
if ($outputDir -and -not (Test-Path $outputDir)) {
    New-Item -ItemType Directory -Force -Path $outputDir | Out-Null
}
Set-Content -Path $Output -Value $reportText -Encoding UTF8
Write-Host "StackOBot gameplay logic report written: $Output"

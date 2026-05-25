# Poll node0 JSON-RPC until L3 macro finality is observable (spec §8 E2E).
# Requires four-node docker compose with node0 RPC mapped to host port 9200.

param(
    [string]$RpcUrl = "http://127.0.0.1:9200/",
    [int]$FinalizeTimeoutSecs = 300,
    [int]$PollIntervalSecs = 5
)

function Invoke-LuaRpc {
    param([string]$Method, [object[]]$Params = @())
    $body = @{
        jsonrpc = "2.0"
        id      = 1
        method  = $Method
        params  = $Params
    } | ConvertTo-Json -Compress
    return Invoke-RestMethod -Uri $RpcUrl -Method Post -ContentType "application/json" -Body $body
}

Write-Host "devnet E2E: polling $RpcUrl for lua_getLatestFinalized (timeout ${FinalizeTimeoutSecs}s)"
$deadline = (Get-Date).AddSeconds($FinalizeTimeoutSecs)
$attempt = 0

while ($true) {
    $attempt++
    try {
        $resp = Invoke-LuaRpc -Method "lua_getLatestFinalized"
        $hash = $resp.result.checkpoint_hash
        if ($null -ne $hash -and $hash -ne "") {
            $mode = $resp.result.mode
            Write-Host "consensus progressed: checkpoint_hash=$hash mode=$mode"
            $mc = Invoke-LuaRpc -Method "lua_getMacroCheckpointAt" -Params @(1)
            if ($null -ne $mc.result.checkpoint_borsh_hex) {
                $len = $mc.result.checkpoint_borsh_hex.Length
                Write-Host "macro checkpoint at height 1 present ($len hex chars)"
            } else {
                Write-Host "warning: lua_getMacroCheckpointAt(1) returned null (finalized QC still OK)"
            }
            exit 0
        }
        Write-Host "attempt ${attempt}: latest_finalized still null; retrying in ${PollIntervalSecs}s"
    } catch {
        Write-Host "attempt ${attempt}: RPC unreachable ($($_.Exception.Message)); retrying in ${PollIntervalSecs}s"
    }

    if ((Get-Date) -gt $deadline) {
        Write-Error "Timed out after ${FinalizeTimeoutSecs}s waiting for macro finality"
        exit 1
    }
    Start-Sleep -Seconds $PollIntervalSecs
}

use crate::pty::emulator::SCROLLBACK_LIMIT;
use crate::pty::manager::{relay_task, ProcInfo};
use crate::pty::{PtyManager, PtySnapshot, PtySpawnConfig, SessionId};
use std::sync::Arc;
use tauri::{AppHandle, Manager, State};

#[tauri::command]
pub fn pty_spawn(
    id: SessionId,
    config: PtySpawnConfig,
    manager: State<'_, Arc<PtyManager>>,
    app: AppHandle,
) -> Result<SessionId, String> {
    // Guard OmniFS (F2 item 7): cwd dentro do mount FUSE conhecido com o daemon
    // morto → erro claro AQUI (o nó mostra a mensagem via setError) em vez de um
    // terminal nascendo num filesystem desconectado (todo IO daria ENOTCONN).
    // Choke-point único: cobre Sidebar, restore, pipeline e mobile. Barato —
    // 1 JSON pequeno + 1 connect local, só quando o cwd bate no prefixo do mount.
    crate::omnifs::preflight_cwd_guard(config.cwd.as_deref())?;
    
    let label = config.label.clone();
    let role = config.role.clone();
    let floor = config.cwd.as_ref().and_then(|p| {
        std::path::Path::new(p)
            .file_name()
            .and_then(|n| n.to_str())
            .map(String::from)
    });
    
    let result = manager.spawn(id.clone(), config, app.clone());
    
    // Registry é `Arc<AgentRegistry>` (manage no boot) — try_state evita panic se
    // o state ainda não estiver disponível; fail-soft (PTY sobe mesmo sem registry).
    if result.is_ok() {
        if let Some(label_str) = label {
            if let Some(registry) = app.try_state::<Arc<crate::mcp::AgentRegistry>>() {
                let desc = format!("{} (PTY)", role.as_deref().unwrap_or("agente"));
                registry.register_with_role(label_str, id.clone(), desc, floor, role);
            }
        }
    }
    
    result.map_err(|e| format!("{e:#}"))
}

#[tauri::command]
pub fn pty_write(
    session_id: SessionId,
    data: String,
    manager: State<'_, std::sync::Arc<PtyManager>>,
) -> Result<(), String> {
    manager.write(&session_id, data.as_bytes()).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
pub fn pty_resize(
    session_id: SessionId,
    cols: u16,
    rows: u16,
    manager: State<'_, std::sync::Arc<PtyManager>>,
) -> Result<(), String> {
    manager.resize(&session_id, cols, rows).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
pub fn pty_kill(
    session_id: SessionId,
    manager: State<'_, Arc<PtyManager>>,
    app: AppHandle,
) -> Result<(), String> {
    if let Some(registry) = app.try_state::<Arc<crate::mcp::AgentRegistry>>() {
        let removed = registry.unregister_by_session(&session_id);
        if !removed.is_empty() {
            log::info!(
                "MCP: agentes desregistrados no kill de {}: {:?}",
                &session_id[..8.min(session_id.len())],
                removed
            );
        }
    }
    manager.kill(&session_id).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
pub fn pty_list(manager: State<'_, std::sync::Arc<PtyManager>>) -> Vec<SessionId> {
    manager.list()
}

/// Só as sessões cujo processo AINDA RODA. `pty_list` continua devolvendo todas (o
/// scrollback de uma sessão morta ainda é consultável); quem vai ATTACHAR usa esta —
/// colar num cadáver deixava o terminal em branco, sem erro nenhum, com o card verde.
#[tauri::command]
pub fn pty_list_alive(manager: State<'_, std::sync::Arc<PtyManager>>) -> Vec<SessionId> {
    manager.list_alive()
}

/// PID + RSS do processo de uma sessão (process mgmt na UI). None se sumiu.
#[tauri::command]
pub fn pty_proc_info(
    session_id: SessionId,
    manager: State<'_, std::sync::Arc<PtyManager>>,
) -> Option<ProcInfo> {
    manager.proc_info(&session_id)
}

/// BATCH: PID + RSS de TODAS as sessões num só invoke (chave = session_id). Substitui
/// N chamadas `pty_proc_info` (1 por node) por 1 só — o hook singleton `useProcInfo`
/// distribui pros nodes. Menos IPC + menos re-render no tick de recursos.
#[tauri::command]
pub fn pty_proc_info_all(
    manager: State<'_, std::sync::Arc<PtyManager>>,
) -> std::collections::HashMap<SessionId, ProcInfo> {
    manager.proc_info_all()
}

/// Tela renderizada (VT100) de uma sessão — usada pra semear o espelho do
/// Orquestrador no dock sem re-spawnar a sessão.
#[tauri::command]
pub fn pty_read_screen(
    session_id: SessionId,
    manager: State<'_, std::sync::Arc<PtyManager>>,
) -> Result<String, String> {
    manager.read_screen(&session_id).map_err(|e| format!("{e:#}"))
}

/// Snapshot serializado (scrollback+viewport em ANSI re-hidratado) do emulador VT
/// headless de uma sessão (ref P0 #2). O front chama no retorno-de-oculto / overflow
/// pra re-hidratar a view e dedupar os chunks ao vivo por `seq`. Erro se a sessão não
/// tem emulador → o front degrada pro fluxo ao vivo atual (não quebra).
#[tauri::command]
pub fn pty_snapshot(
    session_id: SessionId,
    manager: State<'_, std::sync::Arc<PtyManager>>,
) -> Result<PtySnapshot, String> {
    manager
        .snapshot(&session_id, SCROLLBACK_LIMIT)
        .map_err(|e| format!("{e:#}"))
}

#[tauri::command]
pub async fn pty_pipe_create(
    source_id: SessionId,
    target_id: SessionId,
    source_label: Option<String>,
    manager: State<'_, std::sync::Arc<PtyManager>>,
) -> Result<(), String> {
    let (rx, writer) = manager
        .pipe_parts(&source_id, &target_id)
        .map_err(|e| format!("{e:#}"))?;
    let label = source_label.unwrap_or_else(|| source_id.clone());
    let handle = tokio::spawn(relay_task(rx, writer, source_id.clone(), target_id.clone(), label));
    manager.pipe_store(source_id, target_id, handle);
    Ok(())
}

#[tauri::command]
pub fn pty_pipe_remove(
    source_id: SessionId,
    target_id: SessionId,
    manager: State<'_, std::sync::Arc<PtyManager>>,
) -> Result<(), String> {
    manager.pipe_remove(&source_id, &target_id).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
pub fn pty_pipe_list(manager: State<'_, std::sync::Arc<PtyManager>>) -> Vec<[SessionId; 2]> {
    manager.pipe_list()
}

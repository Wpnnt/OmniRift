use crate::pty::SessionId;
use dashmap::DashMap;
use std::sync::Arc;

#[derive(Clone)]
pub struct AgentEntry {
    pub session_id: SessionId,
    pub description: String,
    /// Nome do floor onde o agente vive — dá ao Orquestrador a topologia
    /// cross-floor (quem está em qual branch). `None` = floor desconhecido.
    pub floor: Option<String>,
    /// CLI/role do agente (opencode/claude-code/codex/shell). Usado pelo
    /// resolve_group pra casar @role:X (ex: @role:opencode → todos opencode).
    pub role: Option<String>,
}

/// Mapeia label de agente → (session_id PTY, description, floor).
/// Cada agente registrado vira uma tool dinâmica no MCP.
#[derive(Default, Clone)]
pub struct AgentRegistry(Arc<DashMap<String, AgentEntry>>);

impl AgentRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(
        &self,
        label: String,
        session_id: SessionId,
        description: String,
        floor: Option<String>,
    ) {
        self.register_with_role(label, session_id, description, floor, None);
    }

    pub fn register_with_role(
        &self,
        label: String,
        session_id: SessionId,
        description: String,
        floor: Option<String>,
        role: Option<String>,
    ) {
        // Re-registro (autoRegister / toggle MCP / restore) costuma vir SEM role.
        // Preserva o role já gravado pelo pty_spawn (opencode/claude-code/…) —
        // senão o log fica "role=Some → role=None" e @role:X deixa de casar.
        let role = role.or_else(|| self.0.get(&label).and_then(|e| e.role.clone()));
        let floor = floor.or_else(|| self.0.get(&label).and_then(|e| e.floor.clone()));
        log::info!("MCP: agente '{}' registrado ({}, role={:?})", label, &session_id[..8.min(session_id.len())], role);
        self.0.insert(label, AgentEntry { session_id, description, floor, role });
    }

    pub fn unregister(&self, label: &str) -> Option<SessionId> {
        self.0.remove(label).map(|(_, e)| e.session_id)
    }

    /// Renomeia o agente addressável pela sessão (UI rename no canvas).
    /// Move a entry `old_label → new_label` mantendo session/description/floor/role.
    /// Se `new_label` já existir com OUTRA sessão, falha soft (não sobrescreve).
    /// Retorna o label antigo se moveu; `None` se a sessão não estava no registry.
    pub fn rename_by_session(&self, session_id: &str, new_label: &str) -> Option<String> {
        let new_label = new_label.trim();
        if new_label.is_empty() {
            return None;
        }
        let old_labels: Vec<String> = self
            .0
            .iter()
            .filter(|e| e.value().session_id == session_id)
            .map(|e| e.key().clone())
            .collect();
        if old_labels.is_empty() {
            return None;
        }
        // Já está com o nome certo (case-sensitive — o LLM/UI usam o label exato).
        if old_labels.len() == 1 && old_labels[0] == new_label {
            return Some(old_labels[0].clone());
        }
        if let Some(existing) = self.0.get(new_label) {
            if existing.session_id != session_id {
                log::warn!(
                    "MCP: rename recusado — label '{}' já é de outra sessão",
                    new_label
                );
                return None;
            }
        }
        // Pega a entry da primeira (deveria ser 1; se houver fantasma, unifica).
        let mut entry = None;
        for l in &old_labels {
            if let Some((_, e)) = self.0.remove(l) {
                entry = Some(e);
            }
        }
        let entry = entry?;
        let old = old_labels[0].clone();
        log::info!("MCP: agente renomeado '{}' → '{}' ({})", old, new_label, &session_id[..8.min(session_id.len())]);
        self.0.insert(new_label.to_string(), entry);
        Some(old)
    }

    /// Remove TODAS as entries apontando pra esta sessão (uso: sessão morreu/EOF).
    /// Sem isto o label fantasma continua no registry e o resolve fuzzy ainda o
    /// encontra ("dormindo (dead)"). Retorna os labels removidos (pra log).
    pub fn unregister_by_session(&self, session_id: &str) -> Vec<String> {
        let labels: Vec<String> = self
            .0
            .iter()
            .filter(|e| e.value().session_id == session_id)
            .map(|e| e.key().clone())
            .collect();
        for l in &labels {
            self.0.remove(l);
        }
        labels
    }

    pub fn list(&self) -> Vec<(String, AgentEntry)> {
        self.0.iter().map(|e| (e.key().clone(), e.value().clone())).collect()
    }

    pub fn get_session_id(&self, label: &str) -> Option<SessionId> {
        self.0.get(label).map(|e| e.session_id.clone())
    }

    /// Busca agente pelo nome de tool MCP (label normalizado em snake_case).
    pub fn get_by_tool_name(&self, tool_name: &str) -> Option<(String, AgentEntry)> {
        self.0
            .iter()
            .find(|e| to_tool_name(e.key()) == tool_name)
            .map(|e| (e.key().clone(), e.value().clone()))
    }
}

/// Converte label de agente em nome de tool MCP válido.
/// "Agente 01" → "agente_01" | "Frontend (React)" → "frontend_react"
pub fn to_tool_name(label: &str) -> String {
    label
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect::<String>()
        .split('_')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("_")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rename_by_session_moves_label() {
        let reg = AgentRegistry::new();
        reg.register_with_role(
            "OpenCode".into(),
            "sid-1".into(),
            "desc".into(),
            Some("main".into()),
            Some("opencode".into()),
        );
        let old = reg.rename_by_session("sid-1", "Agent1");
        assert_eq!(old.as_deref(), Some("OpenCode"));
        assert!(reg.get_session_id("OpenCode").is_none());
        assert_eq!(reg.get_session_id("Agent1").as_deref(), Some("sid-1"));
        let list = reg.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].0, "Agent1");
        assert_eq!(list[0].1.role.as_deref(), Some("opencode"));
    }

    #[test]
    fn rename_by_session_idempotent_same_label() {
        let reg = AgentRegistry::new();
        reg.register("Agent1".into(), "sid-1".into(), "d".into(), None);
        assert_eq!(reg.rename_by_session("sid-1", "Agent1").as_deref(), Some("Agent1"));
        assert_eq!(reg.list().len(), 1);
    }

    #[test]
    fn rename_by_session_refuses_collision() {
        let reg = AgentRegistry::new();
        reg.register("A".into(), "sid-1".into(), "d".into(), None);
        reg.register("B".into(), "sid-2".into(), "d".into(), None);
        assert!(reg.rename_by_session("sid-1", "B").is_none());
        assert_eq!(reg.get_session_id("A").as_deref(), Some("sid-1"));
        assert_eq!(reg.get_session_id("B").as_deref(), Some("sid-2"));
    }

    #[test]
    fn rename_by_session_unknown_is_none() {
        let reg = AgentRegistry::new();
        assert!(reg.rename_by_session("missing", "X").is_none());
    }
}

# OpenCode orquestrável — STATUS da sessão 2026-07-16

> Documento de progresso: o que foi feito, o que está validado e o que ainda falta.
> Branch: **`feature/opencode-alternative`**

---

## 1. O que é

Integração do **OpenCode como agente orquestrável** no OmniRift: spawn via MCP, registro no orquestrador, comunicação peer-to-peer com outros agentes (via `agent_ask`/`agent_tell`), e UI para toggle OpenCode/Claude nos Roles.

---

## 2. O que foi FEITO

### Registry + spawn OpenCode
- **`mcp/registry.rs`**: campo `engine_type: Option<String>` no `AgentEntry` para distinguir `"opencode"` vs `"claude"` (default).
- **`commands/mcp.rs::spawn_opencode_agent`**: spawna opencode via `pty_spawn` com label registrado (`@Frontend`, `@Backend`, etc.), config via `OPENCODE_CONFIG` (JSON inline com `mcpServers` do agente).
- **CLI `opencode`**: path `C:\\Users\\Warph11\\AppData\\Local\\Microsoft\\WinGet\\Links\\opencode.exe` (symlink WinGet).

### Roles + UI toggle
- **`Sidebar.tsx`**:
  - Toggle **OpenCode / Claude** por role (botão UI acima do role text).
  - State `roleEngines: Record<string, 'claude' | 'opencode'>` no localStorage.
  - Ao criar agente: se engine=`opencode` → chama `spawnOpencodeAgent`; senão `spawnClaudeAgent` (fluxo original).
- **Bug corrigido**: toggle apagava o `roleText` — fix: `setRoleText(prev => prev || roles[...].content)`.

### MCP só em Roles
- **Limitação identificada**: `orchestration-client.ts::spawnAgent` (spawn direto via Constructor, sem role) NÃO passa config MCP. OpenCode só funciona via **Roles** (botão "Create Agent" na sidebar).

### Problemas investigados
1. **Panic `session_id not in sessions`**: causa raiz era pipe stderr não-consumido bloqueando o writer do child. **Fix**: `session.rs` drena stderr em bg thread separado (já commitado em sessão anterior).
2. **Pipe vs orquestração**: usar `pty_connect_pipe` entre 2 agentes OpenCode **não** dá orquestração (stdin/stdout cruzados, sem MCP). Para orquestração real, precisa das **3 tools** (`agent_ask`, `agent_tell`, `agent_status`) via MCP.

---

## 3. O que FALTA

### Choke-point: spawn direto (Constructor) sem MCP
- [ ] **`orchestration-client.ts::spawnAgent`** (spawn pelo Constructor, sem role) precisa receber `mcpConfig` opcional e:
  - Se `engine=opencode` + `mcpConfig` presente → chamar `commands::spawn_opencode_agent`.
  - Senão → fluxo `pty_spawn` original (Claude ou shell).
- [ ] **`Sidebar.tsx`**: ao spawnar via Constructor (fora de role), ler `roleEngines[roleId]` e montar o `mcpConfig` para passar ao `spawnAgent`.

### OpenCode com MCP via spawn direto
- [ ] Garantir que **qualquer** spawn de OpenCode (role ou Constructor) injete o MCP do orquestrador (`omnirift-agents` server local).
- [ ] Testar spawn direto: Constructor → drag pro canvas → agente OpenCode registrado + responde a `agent_ask`.

### Validação manual pendente
- [ ] 2 agentes OpenCode no canvas (ambos via Roles): `@Frontend agent_ask @Backend "qual é o backend?"` → resposta natural.
- [ ] 1 Claude + 1 OpenCode: Claude pergunta ao OpenCode via `agent_ask` → resposta.
- [ ] Spawn direto (Constructor, sem role): OpenCode spawnado + registrado + responde.

---

## 4. Como rodar/testar (máquina de dev — Windows)

```powershell
# Dev (abre app + hot-reload)
cd C:\vault_dev\dev_code\1.development\OmniRift
npm run tauri:dev

# Typecheck (após mudanças TS)
npm run typecheck

# Cargo check (após mudanças Rust)
cd apps\desktop\src-tauri
cargo check -p omnirift
```

### Roteiro de validação manual (OpenCode via Roles)
1. Abrir sidebar → Roles → criar role "Frontend" com engine **OpenCode**.
2. "Create Agent" → agente aparece no canvas, terminal abre.
3. No Constructor: `@Frontend agent_status` → mostra estado do agente.
4. Criar role "Backend" (OpenCode) + agente.
5. No Constructor: `@Frontend agent_ask @Backend "o que você faz?"` → Backend responde.

---

## 5. Arquivos tocados (mudanças unstaged em `feature/opencode-alternative`)

```
apps/desktop/src-tauri/src/commands/mcp.rs          spawn_opencode_agent
apps/desktop/src-tauri/src/mcp/registry.rs          engine_type field
apps/desktop/src-tauri/src/mcp/tools.rs             (ajustes menores)
apps/desktop/src/components/Sidebar.tsx             toggle OpenCode/Claude + roleEngines
apps/desktop/src/lib/mcp-client.ts                  spawnOpencodeAgent
apps/desktop/src/lib/orchestration-client.ts        (falta: mcpConfig no spawnAgent)
```

---

## 6. Decisões pendentes

1. **MCP config no spawn direto** (Constructor sem role) — estrutura `mcpConfig` opcional no `spawnAgent` + command Tauri correspondente, ou forçar que OpenCode só funcione via Roles?
2. **Default engine** — hoje é Claude; manter ou deixar o usuário escolher global?
3. **Validação de path** — `opencode.exe` hardcoded no WinGet path; adicionar fallback ou config?

---

## 7. Próximo passo de código

**Choke-point:** `pty_spawn` direto (sem role) precisa injetar MCP se `engine=opencode`.

1. Adicionar `mcpConfig?: string` (JSON) no `orchestration-client.ts::spawnAgent`.
2. Se `engine=opencode` + `mcpConfig` → `invoke('spawn_opencode_agent', {label, config})`.
3. `Sidebar.tsx`: ao spawnar pelo Constructor, montar o `mcpConfig` (MCP do orquestrador) e passar.
4. Testar spawn direto: Constructor → drag → agente OpenCode registrado + MCP ativo.

---

## 8. Referências

- **Spec orquestração**: `docs/superpowers/specs/2026-07-09-orquestracao-design.md`
- **Plano orquestração**: `docs/superpowers/plans/2026-07-09-orquestracao.md`
- **STATUS orquestração (sessão anterior)**: `docs/superpowers/2026-07-09-orquestracao-STATUS.md`
- **OpenCode CLI**: `https://opencode.ai`
- **MCP spec**: `https://spec.modelcontextprotocol.io/`

import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";

type ApiProvider = "openai" | "anthropic" | "elevenlabs" | "ollama" | "custom";
type ModelType = "text" | "vision" | "tts";

interface ApiConfig {
  id: string;
  name: string;
  provider: ApiProvider;
  model_type: ModelType;
  base_url: string;
  model_name: string;
  api_key?: string | null;
  api_key_ref?: string | null;
  language?: string | null;
  is_local: boolean;
  is_active: boolean;
  created_at?: string | null;
}

const PROVIDERS: ApiProvider[] = ["openai", "anthropic", "elevenlabs", "ollama", "custom"];
const MODEL_TYPES: ModelType[] = ["text", "vision", "tts"];

const providerLabels: Record<ApiProvider, string> = {
  openai: "OpenAI",
  anthropic: "Anthropic",
  elevenlabs: "ElevenLabs",
  ollama: "Ollama",
  custom: "Custom",
};

function emptyConfig(): ApiConfig {
  return {
    id: crypto.randomUUID(),
    name: "",
    provider: "openai",
    model_type: "text",
    base_url: "",
    model_name: "",
    api_key: "",
    language: null,
    is_local: false,
    is_active: false,
  };
}

interface ApiConfigPageProps {
  onBack: () => void;
}

export default function ApiConfigPage({ onBack }: ApiConfigPageProps) {
  const { t } = useTranslation();
  const [configs, setConfigs] = useState<ApiConfig[]>([]);
  const [editing, setEditing] = useState<ApiConfig | null>(null);
  const [testResult, setTestResult] = useState<string | null>(null);
  const [testing, setTesting] = useState(false);

  const load = async () => {
    try {
      const list = await invoke<ApiConfig[]>("get_api_configs");
      setConfigs(list);
    } catch (e) {
      console.error("get_api_configs failed:", e);
    }
  };

  useEffect(() => { load(); }, []);

  const handleSave = async () => {
    if (!editing) return;
    try {
      await invoke("save_api_config", { config: editing });
      setEditing(null);
      await load();
    } catch (e) {
      console.error("save_api_config failed:", e);
    }
  };

  const handleDelete = async (id: string) => {
    if (!confirm(t("apiConfig.confirmDelete"))) return;
    try {
      await invoke("delete_api_config_cmd", { id });
      await load();
    } catch (e) {
      console.error("delete_api_config_cmd failed:", e);
    }
  };

  const handleTest = async () => {
    if (!editing) return;
    setTesting(true);
    setTestResult(null);
    try {
      const ok = await invoke<boolean>("test_api_connection", { config: editing });
      setTestResult(ok ? t("apiConfig.connectionSuccess") : t("apiConfig.connectionFailed"));
    } catch (e) {
      setTestResult(t("apiConfig.connectionFailed") + ": " + String(e));
    } finally {
      setTesting(false);
    }
  };

  const modelTypeLabel = (mt: ModelType) => {
    const map: Record<ModelType, string> = {
      text: t("apiConfig.modelTypeText"),
      vision: t("apiConfig.modelTypeVision"),
      tts: t("apiConfig.modelTypeTts"),
    };
    return map[mt];
  };

  // ── Edit form ──
  if (editing) {
    return (
      <div className="api-config-page">
        <button className="settings-btn back-btn" onClick={() => { setEditing(null); setTestResult(null); }}>
          ← {t("common.back")}
        </button>
        <h2 className="settings-title">{editing.created_at ? t("apiConfig.edit") : t("apiConfig.add")}</h2>

        <label className="settings-row">
          <span className="settings-label">{t("apiConfig.name")}</span>
          <input className="settings-input-text" value={editing.name} onChange={(e) => setEditing({ ...editing, name: e.target.value })} />
        </label>

        <label className="settings-row">
          <span className="settings-label">{t("apiConfig.provider")}</span>
          <select className="settings-select" value={editing.provider} onChange={(e) => setEditing({ ...editing, provider: e.target.value as ApiProvider, is_local: e.target.value === "ollama" })}>
            {PROVIDERS.map((p) => <option key={p} value={p}>{providerLabels[p]}</option>)}
          </select>
        </label>

        <label className="settings-row">
          <span className="settings-label">{t("apiConfig.modelType")}</span>
          <select className="settings-select" value={editing.model_type} onChange={(e) => setEditing({ ...editing, model_type: e.target.value as ModelType })}>
            {MODEL_TYPES.map((mt) => <option key={mt} value={mt}>{modelTypeLabel(mt)}</option>)}
          </select>
        </label>

        <label className="settings-row">
          <span className="settings-label">{t("apiConfig.baseUrl")}</span>
          <input className="settings-input-text" value={editing.base_url} onChange={(e) => setEditing({ ...editing, base_url: e.target.value })} placeholder="https://api.openai.com/v1" />
        </label>

        <label className="settings-row">
          <span className="settings-label">{t("apiConfig.modelName")}</span>
          <input className="settings-input-text" value={editing.model_name} onChange={(e) => setEditing({ ...editing, model_name: e.target.value })} placeholder="gpt-4o" />
        </label>

        <label className="settings-row">
          <span className="settings-label">{t("apiConfig.apiKey")}</span>
          <input className="settings-input-text" type="password" value={editing.api_key ?? ""} onChange={(e) => setEditing({ ...editing, api_key: e.target.value })} placeholder="sk-..." />
        </label>

        {editing.model_type === "tts" && (
          <label className="settings-row">
            <span className="settings-label">{t("apiConfig.language")}</span>
            <input className="settings-input-text" value={editing.language ?? ""} onChange={(e) => setEditing({ ...editing, language: e.target.value || null })} placeholder="en / zh / ja" />
          </label>
        )}

        <label className="settings-row">
          <span className="settings-label">{t("apiConfig.isLocal")}</span>
          <input type="checkbox" checked={editing.is_local} onChange={(e) => setEditing({ ...editing, is_local: e.target.checked })} />
        </label>

        <div className="api-config-actions">
          <button className="settings-btn" onClick={handleTest} disabled={testing}>
            {testing ? t("apiConfig.testing") : t("apiConfig.testConnection")}
          </button>
          <button className="settings-btn primary" onClick={handleSave}>{t("common.save")}</button>
          <button className="settings-btn" onClick={() => { setEditing(null); setTestResult(null); }}>{t("common.cancel")}</button>
        </div>
        {testResult && <p className="api-config-test-result">{testResult}</p>}
      </div>
    );
  }

  // ── List view ──
  return (
    <div className="api-config-page">
      <button className="settings-btn back-btn" onClick={onBack}>← {t("common.back")}</button>
      <h2 className="settings-title">{t("apiConfig.title")}</h2>

      <button className="settings-btn primary add-btn" onClick={() => setEditing(emptyConfig())}>
        + {t("apiConfig.add")}
      </button>

      {configs.length === 0 && <p className="empty-hint">{t("apiConfig.noConfigs")}</p>}

      <ul className="api-config-list" role="list">
        {configs.map((c) => (
          <li key={c.id} className="api-config-item">
            <div className="api-config-item-info">
              <strong>{c.name || c.id}</strong>
              <span className="api-config-meta">{providerLabels[c.provider]} · {modelTypeLabel(c.model_type)}{c.language ? ` · ${c.language}` : ""}</span>
            </div>
            <div className="api-config-item-actions">
              <button className="settings-btn" onClick={() => setEditing({ ...c, api_key: "" })}>{t("apiConfig.edit")}</button>
              <button className="settings-btn danger" onClick={() => handleDelete(c.id)}>{t("apiConfig.delete")}</button>
            </div>
          </li>
        ))}
      </ul>
    </div>
  );
}

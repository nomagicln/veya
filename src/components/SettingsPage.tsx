import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { useAppStore, type AppSettings } from "../store";

interface SettingsPageProps {
  onNavigateApiConfig: () => void;
}

export default function SettingsPage({ onNavigateApiConfig }: SettingsPageProps) {
  const { t, i18n } = useTranslation();
  const storeSettings = useAppStore((s) => s.settings);
  const updateStoreSettings = useAppStore((s) => s.updateSettings);
  const setLocale = useAppStore((s) => s.setLocale);

  const [settings, setSettings] = useState<AppSettings>(storeSettings);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    (async () => {
      try {
        const s = await invoke<AppSettings>("get_settings");
        setSettings(s);
        updateStoreSettings(s);
      } catch (e) {
        console.error("get_settings failed:", e);
      } finally {
        setLoading(false);
      }
    })();
  }, [updateStoreSettings]);

  const save = async (patch: Partial<AppSettings>) => {
    const next = { ...settings, ...patch };
    setSettings(next);
    updateStoreSettings(patch);
    try {
      await invoke("update_settings", { settings: next });
    } catch (e) {
      console.error("update_settings failed:", e);
    }
  };

  const handleLocaleChange = async (locale: string) => {
    await save({ locale });
    i18n.changeLanguage(locale);
    setLocale(locale);
  };

  if (loading) {
    return <p className="settings-loading">{t("common.loading")}</p>;
  }

  return (
    <div className="settings-page">
      <h2 className="settings-title">{t("settings.title")}</h2>

      {/* AI Completion toggle */}
      <label className="settings-row">
        <span className="settings-label">{t("settings.aiCompletion")}</span>
        <input
          type="checkbox"
          checked={settings.aiCompletionEnabled}
          onChange={(e) => save({ aiCompletionEnabled: e.target.checked })}
          aria-label={t("settings.aiCompletion")}
        />
      </label>
      <p className="settings-hint">{t("settings.aiCompletionDesc")}</p>

      {/* Cache settings */}
      <label className="settings-row">
        <span className="settings-label">{t("settings.cacheMaxSize")}</span>
        <input
          type="number"
          min={50}
          max={10000}
          value={settings.cacheMaxSizeMb}
          onChange={(e) => save({ cacheMaxSizeMb: Number(e.target.value) || 500 })}
          className="settings-input-number"
        />
      </label>

      <label className="settings-row">
        <span className="settings-label">{t("settings.cacheAutoCleanDays")}</span>
        <input
          type="number"
          min={1}
          max={365}
          value={settings.cacheAutoCleanDays}
          onChange={(e) => save({ cacheAutoCleanDays: Number(e.target.value) || 30 })}
          className="settings-input-number"
        />
      </label>

      {/* Retry count */}
      <label className="settings-row">
        <span className="settings-label">{t("settings.retryCount")}</span>
        <input
          type="number"
          min={0}
          max={10}
          value={settings.retryCount}
          onChange={(e) => save({ retryCount: Number(e.target.value) || 3 })}
          className="settings-input-number"
        />
      </label>

      {/* Shortcut */}
      <label className="settings-row">
        <span className="settings-label">{t("settings.shortcutCapture")}</span>
        <input
          type="text"
          value={settings.shortcutCapture}
          onChange={(e) => save({ shortcutCapture: e.target.value })}
          className="settings-input-text"
        />
      </label>

      {/* Language */}
      <label className="settings-row">
        <span className="settings-label">{t("settings.language")}</span>
        <select
          value={settings.locale}
          onChange={(e) => handleLocaleChange(e.target.value)}
          className="settings-select"
          aria-label={t("settings.language")}
        >
          <option value="zh-CN">中文</option>
          <option value="en-US">English</option>
        </select>
      </label>

      {/* API Config entry */}
      <div className="settings-row">
        <span className="settings-label">{t("settings.apiConfig")}</span>
        <button className="settings-btn" onClick={onNavigateApiConfig}>
          {t("apiConfig.title")} →
        </button>
      </div>
    </div>
  );
}

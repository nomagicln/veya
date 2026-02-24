import { useEffect, useState, useCallback, useRef } from "react";
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
      if (patch.shortcutCapture) {
        await invoke("update_capture_shortcut", { shortcut: patch.shortcutCapture });
      }
    } catch (e) {
      console.error("update_settings failed:", e);
    }
  };

  const handleLocaleChange = async (locale: string) => {
    await save({ locale });
    i18n.changeLanguage(locale);
    setLocale(locale);
  };

  // --- Shortcut recorder ---
  const [recording, setRecording] = useState(false);
  const [pressedKeys, setPressedKeys] = useState<Set<string>>(new Set());
  const shortcutInputRef = useRef<HTMLButtonElement>(null);
  const heldRef = useRef<Set<string>>(new Set());
  const saveRef = useRef(save);
  saveRef.current = save;

  const MODIFIERS = ["CommandOrControl", "Shift", "Alt"];

  const keyToTauriToken = useCallback((e: KeyboardEvent): string | null => {
    const { key, code } = e;
    if (key === "Meta" || key === "Control") return "CommandOrControl";
    if (key === "Shift") return "Shift";
    if (key === "Alt") return "Alt";
    if (/^Key([A-Z])$/.test(code)) return code.replace("Key", "");
    if (/^Digit(\d)$/.test(code)) return code.replace("Digit", "");
    if (/^F(\d+)$/.test(code)) return code;
    const specials: Record<string, string> = {
      Space: "Space", Enter: "Enter", Escape: "Escape",
      ArrowUp: "Up", ArrowDown: "Down", ArrowLeft: "Left", ArrowRight: "Right",
      Backspace: "Backspace", Delete: "Delete", Tab: "Tab",
      Home: "Home", End: "End", PageUp: "PageUp", PageDown: "PageDown",
      BracketLeft: "[", BracketRight: "]", Backslash: "\\",
      Semicolon: ";", Quote: "'", Comma: ",", Period: ".", Slash: "/",
      Minus: "-", Equal: "=", Backquote: "`",
    };
    return specials[code] ?? null;
  }, []);

  const formatShortcut = useCallback((keys: Set<string>): string => {
    const order = ["CommandOrControl", "Shift", "Alt"];
    const modifiers = order.filter((m) => keys.has(m));
    const others = [...keys].filter((k) => !order.includes(k));
    return [...modifiers, ...others].join("+");
  }, []);

  useEffect(() => {
    if (!recording) {
      heldRef.current = new Set();
      return;
    }

    heldRef.current = new Set();

    const onKeyDown = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      const token = keyToTauriToken(e);
      if (!token) return;

      heldRef.current.add(token);
      setPressedKeys(new Set(heldRef.current));

      // If we have at least one non-modifier key, finalize immediately
      const hasNonModifier = [...heldRef.current].some((k) => !MODIFIERS.includes(k));
      if (hasNonModifier) {
        const combo = formatShortcut(heldRef.current);
        saveRef.current({ shortcutCapture: combo });
        setRecording(false);
        setPressedKeys(new Set());
      }
    };

    const onBlur = () => {
      setRecording(false);
      setPressedKeys(new Set());
    };

    window.addEventListener("keydown", onKeyDown, true);
    window.addEventListener("blur", onBlur);

    return () => {
      window.removeEventListener("keydown", onKeyDown, true);
      window.removeEventListener("blur", onBlur);
    };
  }, [recording, keyToTauriToken, formatShortcut]);

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

      {/* Shortcut recorder */}
      <div className="settings-row">
        <span className="settings-label">{t("settings.shortcutCapture")}</span>
        <button
          ref={shortcutInputRef}
          type="button"
          className={`settings-shortcut-btn${recording ? " recording" : ""}`}
          onClick={() => {
            setRecording(true);
            setPressedKeys(new Set());
          }}
          aria-label={t("settings.shortcutCapture")}
        >
          {recording
            ? pressedKeys.size > 0
              ? formatShortcut(pressedKeys)
              : t("settings.shortcutRecording")
            : settings.shortcutCapture || t("settings.shortcutRecording")}
        </button>
      </div>

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

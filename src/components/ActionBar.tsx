import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { useAppStore } from "../store";

interface ActionBarProps {
  onGeneratePodcast: () => void;
}

export default function ActionBar({ onGeneratePodcast }: ActionBarProps) {
  const { t } = useTranslation();
  const pinned = useAppStore((s) => s.floatingWindow.pinned);
  const togglePin = useAppStore((s) => s.togglePin);
  const content = useAppStore((s) => s.floatingWindow.currentContent);

  const handleTogglePin = async () => {
    togglePin();
    try {
      await invoke("toggle_pin");
    } catch (e) {
      console.error("toggle_pin failed:", e);
    }
  };

  const handleCopy = async () => {
    if (!content?.sections) return;
    const text = Object.values(content.sections).filter(Boolean).join("\n\n");
    try {
      await navigator.clipboard.writeText(text);
    } catch (e) {
      console.error("copy failed:", e);
    }
  };

  return (
    <div className="action-bar" role="toolbar" aria-label="Actions">
      <button
        className="action-btn"
        onClick={onGeneratePodcast}
        disabled={!content || content.isStreaming}
        aria-label={t("castEngine.generate")}
      >
        🎙️ {t("castEngine.generate")}
      </button>
      <button
        className="action-btn"
        onClick={handleTogglePin}
        aria-label={pinned ? t("floatingWindow.unpin") : t("floatingWindow.pin")}
        aria-pressed={pinned}
      >
        📌 {pinned ? t("floatingWindow.unpin") : t("floatingWindow.pin")}
      </button>
      <button
        className="action-btn"
        onClick={handleCopy}
        disabled={!content}
        aria-label={t("floatingWindow.copy")}
      >
        📋 {t("floatingWindow.copy")}
      </button>
    </div>
  );
}

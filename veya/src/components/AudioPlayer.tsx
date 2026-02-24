import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { useAppStore } from "../store";

export default function AudioPlayer() {
  const { t } = useTranslation();
  const audioState = useAppStore((s) => s.floatingWindow.audioState);
  const updateAudioState = useAppStore((s) => s.updateAudioState);

  if (!audioState) return null;

  const handlePlayPause = () => {
    updateAudioState({ isPlaying: !audioState.isPlaying });
  };

  const handleSave = async () => {
    if (audioState.isSaved) return;
    try {
      await invoke("save_podcast", { tempPath: audioState.audioPath });
      updateAudioState({ isSaved: true });
    } catch (e) {
      console.error("save_podcast failed:", e);
    }
  };

  const progressPercent =
    audioState.duration > 0
      ? (audioState.progress / audioState.duration) * 100
      : 0;

  return (
    <div className="audio-player" role="region" aria-label="Audio Player">
      <button
        className="action-btn"
        onClick={handlePlayPause}
        aria-label={audioState.isPlaying ? t("audioPlayer.pause") : t("audioPlayer.play")}
      >
        {audioState.isPlaying ? "⏸️" : "▶️"}
      </button>
      <div
        className="progress-bar"
        role="progressbar"
        aria-valuenow={Math.round(progressPercent)}
        aria-valuemin={0}
        aria-valuemax={100}
      >
        <div
          className="progress-bar-fill"
          style={{ width: `${progressPercent}%` }}
        />
      </div>
      <button
        className="action-btn"
        onClick={handleSave}
        disabled={audioState.isSaved}
        aria-label={t("audioPlayer.save")}
      >
        💾 {audioState.isSaved ? "✓" : t("audioPlayer.save")}
      </button>
    </div>
  );
}

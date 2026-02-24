import { useEffect, useCallback, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useTranslation } from "react-i18next";
import { useAppStore } from "../store";
import StreamContent from "./StreamContent";
import ActionBar from "./ActionBar";
import AudioPlayer from "./AudioPlayer";
import type { StreamContent as StreamContentType } from "../store";

interface TextInsightChunk {
  type: "start" | "delta" | "done" | "error";
  section?: keyof StreamContentType["sections"];
  content?: string;
  language?: string;
}

interface VisionCaptureChunk {
  type: "ocr_result" | "ai_completion" | "analysis_delta" | "done" | "error";
  content?: string;
  is_ai_inferred?: boolean;
}

interface CastEngineProgress {
  type: "script_generating" | "script_done" | "tts_progress" | "done" | "error";
  progress?: number;
  script_preview?: string;
  audio_path?: string;
  content?: string;
}

/** Map backend error identifiers to i18n keys */
const errorKeyMap: Record<string, string> = {
  InvalidApiKey: "errors.invalidApiKey",
  InsufficientBalance: "errors.insufficientBalance",
  NetworkTimeout: "errors.networkTimeout",
  ModelUnavailable: "errors.modelUnavailable",
  OcrFailed: "errors.ocrFailed",
  TtsFailed: "errors.ttsFailed",
  StorageError: "errors.storageError",
  PermissionDenied: "errors.permissionDenied",
};

function resolveErrorMessage(content: string | undefined, t: (key: string) => string): string {
  if (!content) return t("errors.networkTimeout");
  // Try to match a known error type from the content string
  for (const [key, i18nKey] of Object.entries(errorKeyMap)) {
    if (content.includes(key)) return t(i18nKey);
  }
  return content;
}

/** Save a query record to the backend (fire-and-forget) */
async function saveQueryRecord(contentSnapshot: StreamContentType, detectedLanguage?: string) {
  const inputText = contentSnapshot.sections.original ?? "";
  if (!inputText) return;

  const analysisResult = JSON.stringify(contentSnapshot.sections);
  try {
    await invoke("save_query_record", {
      input: {
        input_text: inputText,
        source: contentSnapshot.source,
        detected_language: detectedLanguage ?? null,
        analysis_result: analysisResult,
      },
    });
  } catch (e) {
    console.error("save_query_record failed:", e);
  }
}

export default function FloatingWindow() {
  const { t } = useTranslation();
  const visible = useAppStore((s) => s.floatingWindow.visible);
  const pinned = useAppStore((s) => s.floatingWindow.pinned);
  const content = useAppStore((s) => s.floatingWindow.currentContent);
  const errorMessage = useAppStore((s) => s.errorMessage);
  const podcastProgress = useAppStore((s) => s.podcastProgress);
  const showWindow = useAppStore((s) => s.showWindow);
  const hideWindow = useAppStore((s) => s.hideWindow);
  const updateContent = useAppStore((s) => s.updateContent);
  const setStreamingSection = useAppStore((s) => s.setStreamingSection);
  const clearContent = useAppStore((s) => s.clearContent);
  const updateAudioState = useAppStore((s) => s.updateAudioState);
  const setError = useAppStore((s) => s.setError);
  const clearError = useAppStore((s) => s.clearError);
  const setPodcastProgress = useAppStore((s) => s.setPodcastProgress);

  // Track detected language from text-insight start events
  const detectedLanguageRef = useRef<string | undefined>(undefined);

  // Handle window blur → auto-hide when not pinned
  const handleBlur = useCallback(async () => {
    if (!pinned) {
      hideWindow();
      try {
        await invoke("hide_floating_window");
      } catch (e) {
        console.error("hide_floating_window failed:", e);
      }
    }
  }, [pinned, hideWindow]);

  useEffect(() => {
    const win = getCurrentWindow();
    const unlisten = win.onFocusChanged(({ payload: focused }) => {
      if (!focused) handleBlur();
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [handleBlur]);

  // Listen to text-insight stream events
  useEffect(() => {
    const unlisten = listen<TextInsightChunk>(
      "veya://text-insight/stream-chunk",
      ({ payload }) => {
        switch (payload.type) {
          case "start":
            clearContent();
            clearError();
            detectedLanguageRef.current = payload.language;
            updateContent({ source: "text_insight", isStreaming: true, sections: {} });
            showWindow();
            break;
          case "delta":
            if (payload.section && payload.content) {
              setStreamingSection(payload.section, payload.content);
            }
            break;
          case "done":
            updateContent({ isStreaming: false });
            // Auto-save learning record
            {
              const snap = useAppStore.getState().floatingWindow.currentContent;
              if (snap) saveQueryRecord(snap, detectedLanguageRef.current);
            }
            break;
          case "error":
            updateContent({ isStreaming: false });
            setError(resolveErrorMessage(payload.content, t));
            break;
        }
      },
    );
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [clearContent, clearError, updateContent, setStreamingSection, showWindow, setError, t]);

  // Listen to vision-capture stream events
  useEffect(() => {
    let accumulatedText = "";
    let aiRanges: Array<{ start: number; end: number }> = [];

    const unlisten = listen<VisionCaptureChunk>(
      "veya://vision-capture/stream-chunk",
      ({ payload }) => {
        switch (payload.type) {
          case "ocr_result":
            accumulatedText = payload.content ?? "";
            aiRanges = [];
            clearContent();
            clearError();
            updateContent({
              source: "vision_capture",
              isStreaming: true,
              sections: { original: accumulatedText },
            });
            showWindow();
            break;
          case "ai_completion":
            if (payload.content) {
              const start = accumulatedText.length;
              accumulatedText += payload.content;
              aiRanges.push({ start, end: accumulatedText.length });
              updateContent({
                sections: { original: accumulatedText },
                aiInferredRanges: [...aiRanges],
              });
            }
            break;
          case "analysis_delta":
            if (payload.content) {
              setStreamingSection("translation", payload.content);
            }
            break;
          case "done":
            updateContent({ isStreaming: false });
            // Auto-save learning record
            {
              const snap = useAppStore.getState().floatingWindow.currentContent;
              if (snap) saveQueryRecord(snap);
            }
            break;
          case "error":
            updateContent({ isStreaming: false });
            setError(resolveErrorMessage(payload.content, t));
            break;
        }
      },
    );
    return () => {
      unlisten.then((fn) => fn());
      accumulatedText = "";
      aiRanges = [];
    };
  }, [clearContent, clearError, updateContent, setStreamingSection, showWindow, setError, t]);

  // Listen to cast-engine progress events
  useEffect(() => {
    const unlisten = listen<CastEngineProgress>(
      "veya://cast-engine/progress",
      ({ payload }) => {
        switch (payload.type) {
          case "script_generating":
            setPodcastProgress({
              stage: "script_generating",
              progress: payload.progress ?? 0,
              scriptPreview: payload.script_preview,
            });
            break;
          case "script_done":
            setPodcastProgress({
              stage: "script_done",
              progress: payload.progress ?? 50,
              scriptPreview: payload.script_preview,
            });
            break;
          case "tts_progress":
            setPodcastProgress({
              stage: "tts_progress",
              progress: payload.progress ?? 0,
            });
            break;
          case "done":
            setPodcastProgress({ stage: "done", progress: 100 });
            if (payload.audio_path) {
              updateAudioState({
                audioPath: payload.audio_path,
                isPlaying: false,
                progress: 0,
                duration: 0,
                isSaved: false,
              });
            }
            break;
          case "error":
            setPodcastProgress({ stage: "error", progress: 0 });
            setError(resolveErrorMessage(payload.content, t));
            break;
        }
      },
    );
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [updateAudioState, setPodcastProgress, setError, t]);

  // Auto-dismiss error after 5 seconds
  useEffect(() => {
    if (!errorMessage) return;
    const timer = setTimeout(() => clearError(), 5000);
    return () => clearTimeout(timer);
  }, [errorMessage, clearError]);

  const handleGeneratePodcast = async () => {
    if (!content) return;
    clearError();
    setPodcastProgress({ stage: "script_generating", progress: 0 });
    const inputContent = Object.values(content.sections).filter(Boolean).join("\n");
    try {
      await invoke("generate_podcast", {
        input: { content: inputContent, source: content.source },
        options: { speed: "Normal", mode: "Bilingual", target_language: "en" },
      });
    } catch (e) {
      console.error("generate_podcast failed:", e);
      setPodcastProgress({ stage: "error", progress: 0 });
      setError(String(e));
    }
  };

  if (!visible) return null;

  const showPodcastStatus =
    podcastProgress.stage !== "idle" && podcastProgress.stage !== "done" && podcastProgress.stage !== "error";

  return (
    <div className="floating-window" role="dialog" aria-label="Veya">
      {errorMessage && (
        <div className="error-banner" role="alert">
          <span>{errorMessage}</span>
          <button
            className="error-dismiss"
            onClick={clearError}
            aria-label={t("common.cancel")}
          >
            ✕
          </button>
        </div>
      )}
      {content && <StreamContent content={content} />}
      {showPodcastStatus && (
        <div className="podcast-progress" role="status">
          <span>
            {podcastProgress.stage === "script_generating" && t("castEngine.scriptGenerating")}
            {podcastProgress.stage === "script_done" && t("castEngine.done")}
            {podcastProgress.stage === "tts_progress" && t("castEngine.ttsProgress")}
          </span>
          {podcastProgress.progress > 0 && (
            <div
              className="progress-bar"
              role="progressbar"
              aria-valuenow={podcastProgress.progress}
              aria-valuemin={0}
              aria-valuemax={100}
            >
              <div
                className="progress-bar-fill"
                style={{ width: `${podcastProgress.progress}%` }}
              />
            </div>
          )}
        </div>
      )}
      <ActionBar onGeneratePodcast={handleGeneratePodcast} />
      <AudioPlayer />
    </div>
  );
}

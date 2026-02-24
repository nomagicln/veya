import { create } from "zustand";

export interface StreamContent {
  source: "text_insight" | "vision_capture";
  sections: {
    original?: string;
    wordByWord?: string;
    structure?: string;
    translation?: string;
    colloquial?: string;
    simplified?: string;
  };
  aiInferredRanges?: Array<{ start: number; end: number }>;
  isStreaming: boolean;
}

export interface AudioPlayerState {
  audioPath: string;
  isPlaying: boolean;
  progress: number;
  duration: number;
  isSaved: boolean;
}

export interface AppSettings {
  aiCompletionEnabled: boolean;
  cacheMaxSizeMb: number;
  cacheAutoCleanDays: number;
  retryCount: number;
  shortcutCapture: string;
  locale: string;
}

export interface FloatingWindowState {
  visible: boolean;
  pinned: boolean;
  position: { x: number; y: number };
  currentContent: StreamContent | null;
  audioState: AudioPlayerState | null;
}

export interface AppState {
  floatingWindow: FloatingWindowState;
  settings: AppSettings;
  locale: string;

  // Error state
  errorMessage: string | null;

  // Podcast generation state
  podcastProgress: {
    stage: 'idle' | 'script_generating' | 'script_done' | 'tts_progress' | 'done' | 'error';
    progress: number;
    scriptPreview?: string;
  };

  // Actions
  showWindow: (position?: { x: number; y: number }) => void;
  hideWindow: () => void;
  togglePin: () => void;
  updateContent: (content: Partial<StreamContent>) => void;
  setStreamingSection: (
    section: keyof StreamContent["sections"],
    value: string,
  ) => void;
  clearContent: () => void;
  updateAudioState: (audio: Partial<AudioPlayerState> | null) => void;
  updateSettings: (settings: Partial<AppSettings>) => void;
  setLocale: (locale: string) => void;
  setError: (message: string) => void;
  clearError: () => void;
  setPodcastProgress: (progress: Partial<AppState['podcastProgress']>) => void;
}

const defaultSettings: AppSettings = {
  aiCompletionEnabled: true,
  cacheMaxSizeMb: 500,
  cacheAutoCleanDays: 30,
  retryCount: 3,
  shortcutCapture: "CommandOrControl+Shift+S",
  locale: "zh-CN",
};

export const useAppStore = create<AppState>((set) => ({
  floatingWindow: {
    visible: false,
    pinned: false,
    position: { x: 0, y: 0 },
    currentContent: null,
    audioState: null,
  },
  settings: defaultSettings,
  locale: "zh-CN",
  errorMessage: null,
  podcastProgress: { stage: 'idle', progress: 0 },

  showWindow: (position) =>
    set((state) => ({
      floatingWindow: {
        ...state.floatingWindow,
        visible: true,
        ...(position ? { position } : {}),
      },
    })),

  hideWindow: () =>
    set((state) => ({
      floatingWindow: { ...state.floatingWindow, visible: false },
    })),

  togglePin: () =>
    set((state) => ({
      floatingWindow: {
        ...state.floatingWindow,
        pinned: !state.floatingWindow.pinned,
      },
    })),

  updateContent: (content) =>
    set((state) => ({
      floatingWindow: {
        ...state.floatingWindow,
        currentContent: state.floatingWindow.currentContent
          ? { ...state.floatingWindow.currentContent, ...content }
          : {
              source: "text_insight",
              sections: {},
              isStreaming: true,
              ...content,
            },
      },
    })),

  setStreamingSection: (section, value) =>
    set((state) => {
      const current = state.floatingWindow.currentContent;
      if (!current) return state;
      return {
        floatingWindow: {
          ...state.floatingWindow,
          currentContent: {
            ...current,
            sections: { ...current.sections, [section]: value },
          },
        },
      };
    }),

  clearContent: () =>
    set((state) => ({
      floatingWindow: {
        ...state.floatingWindow,
        currentContent: null,
        audioState: null,
      },
    })),

  updateAudioState: (audio) =>
    set((state) => ({
      floatingWindow: {
        ...state.floatingWindow,
        audioState: audio
          ? { ...(state.floatingWindow.audioState as AudioPlayerState), ...audio }
          : null,
      },
    })),

  updateSettings: (settings) =>
    set((state) => ({
      settings: { ...state.settings, ...settings },
    })),

  setLocale: (locale) => set({ locale }),

  setError: (message) => set({ errorMessage: message }),

  clearError: () => set({ errorMessage: null }),

  setPodcastProgress: (progress) =>
    set((state) => ({
      podcastProgress: { ...state.podcastProgress, ...progress },
    })),
}));

import { useEffect, useState, useCallback } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";

interface QueryRecord {
  id: string;
  input_text: string;
  source: string;
  detected_language: string | null;
  analysis_result: string;
  created_at: string;
}

interface PodcastRecord {
  id: string;
  input_content: string;
  source: string;
  speed_mode: string;
  podcast_mode: string;
  audio_file_path: string;
  duration_seconds: number | null;
  created_at: string;
}

interface WordFreq {
  word: string;
  language: string;
  count: number;
  last_queried_at: string;
}

type Tab = "query" | "podcast" | "words";
const PAGE_SIZE = 10;

export default function LearningPage() {
  const { t } = useTranslation();
  const [tab, setTab] = useState<Tab>("query");

  const [queries, setQueries] = useState<QueryRecord[]>([]);
  const [queryPage, setQueryPage] = useState(1);
  const [queryHasMore, setQueryHasMore] = useState(true);

  const [podcasts, setPodcasts] = useState<PodcastRecord[]>([]);
  const [podcastPage, setPodcastPage] = useState(1);
  const [podcastHasMore, setPodcastHasMore] = useState(true);

  const [words, setWords] = useState<WordFreq[]>([]);

  const loadQueries = useCallback(async (page: number) => {
    try {
      const rows = await invoke<QueryRecord[]>("get_query_history", { page, pageSize: PAGE_SIZE });
      setQueries(rows);
      setQueryHasMore(rows.length === PAGE_SIZE);
    } catch (e) {
      console.error("get_query_history failed:", e);
    }
  }, []);

  const loadPodcasts = useCallback(async (page: number) => {
    try {
      const rows = await invoke<PodcastRecord[]>("get_podcast_history", { page, pageSize: PAGE_SIZE });
      setPodcasts(rows);
      setPodcastHasMore(rows.length === PAGE_SIZE);
    } catch (e) {
      console.error("get_podcast_history failed:", e);
    }
  }, []);

  const loadWords = useCallback(async () => {
    try {
      const rows = await invoke<WordFreq[]>("get_frequent_words", { limit: 50 });
      setWords(rows);
    } catch (e) {
      console.error("get_frequent_words failed:", e);
    }
  }, []);

  useEffect(() => {
    if (tab === "query") loadQueries(queryPage);
    else if (tab === "podcast") loadPodcasts(podcastPage);
    else loadWords();
  }, [tab, queryPage, podcastPage, loadQueries, loadPodcasts, loadWords]);

  const sourceLabel = (s: string) => {
    const map: Record<string, string> = { text_insight: "📝", vision_capture: "📷", custom: "✏️" };
    return map[s] ?? s;
  };

  return (
    <div className="learning-page">
      <h2 className="settings-title">{t("learningRecord.title")}</h2>

      <div className="learning-tabs" role="tablist">
        {(["query", "podcast", "words"] as Tab[]).map((key) => {
          const labels: Record<Tab, string> = {
            query: t("learningRecord.queryHistory"),
            podcast: t("learningRecord.podcastHistory"),
            words: t("learningRecord.frequentWords"),
          };
          return (
            <button
              key={key}
              role="tab"
              aria-selected={tab === key}
              className={`learning-tab ${tab === key ? "active" : ""}`}
              onClick={() => setTab(key)}
            >
              {labels[key]}
            </button>
          );
        })}
      </div>

      {/* Query history */}
      {tab === "query" && (
        <div role="tabpanel">
          {queries.length === 0 ? (
            <p className="empty-hint">{t("learningRecord.noRecords")}</p>
          ) : (
            <ul className="record-list" role="list">
              {queries.map((q) => (
                <li key={q.id} className="record-item">
                  <span className="record-source">{sourceLabel(q.source)}</span>
                  <span className="record-text">{q.input_text}</span>
                  <time className="record-time">{q.created_at}</time>
                </li>
              ))}
            </ul>
          )}
          <Pagination page={queryPage} hasMore={queryHasMore} onChange={setQueryPage} />
        </div>
      )}

      {/* Podcast history */}
      {tab === "podcast" && (
        <div role="tabpanel">
          {podcasts.length === 0 ? (
            <p className="empty-hint">{t("learningRecord.noRecords")}</p>
          ) : (
            <ul className="record-list" role="list">
              {podcasts.map((p) => (
                <li key={p.id} className="record-item">
                  <span className="record-source">{sourceLabel(p.source)}</span>
                  <span className="record-text">{p.input_content}</span>
                  <span className="record-meta">🎧 {p.audio_file_path.split("/").pop()}</span>
                  <time className="record-time">{p.created_at}</time>
                </li>
              ))}
            </ul>
          )}
          <Pagination page={podcastPage} hasMore={podcastHasMore} onChange={setPodcastPage} />
        </div>
      )}

      {/* Frequent words */}
      {tab === "words" && (
        <div role="tabpanel">
          {words.length === 0 ? (
            <p className="empty-hint">{t("learningRecord.noRecords")}</p>
          ) : (
            <ul className="word-list" role="list">
              {words.map((w) => (
                <li key={w.word + w.language} className="word-item">
                  <span className="word-text">{w.word}</span>
                  <span className="word-count">×{w.count}</span>
                </li>
              ))}
            </ul>
          )}
        </div>
      )}
    </div>
  );
}

function Pagination({ page, hasMore, onChange }: { page: number; hasMore: boolean; onChange: (p: number) => void }) {
  const { t } = useTranslation();
  if (page === 1 && !hasMore) return null;
  return (
    <div className="pagination">
      <button className="settings-btn" disabled={page <= 1} onClick={() => onChange(page - 1)}>
        {t("common.previous")}
      </button>
      <span className="pagination-info">{t("common.page", { page })}</span>
      <button className="settings-btn" disabled={!hasMore} onClick={() => onChange(page + 1)}>
        {t("common.next")}
      </button>
    </div>
  );
}

import { useTranslation } from "react-i18next";
import type { StreamContent as StreamContentType } from "../store";

interface StreamContentProps {
  content: StreamContentType;
}

const sectionKeys = [
  "original",
  "wordByWord",
  "structure",
  "translation",
  "colloquial",
  "simplified",
] as const;

const i18nKeyMap: Record<string, string> = {
  original: "textInsight.original",
  wordByWord: "textInsight.wordByWord",
  structure: "textInsight.structure",
  translation: "textInsight.translation",
  colloquial: "textInsight.colloquial",
  simplified: "textInsight.simplified",
};

export default function StreamContent({ content }: StreamContentProps) {
  const { t } = useTranslation();

  return (
    <div className="stream-content">
      {sectionKeys.map((key) => {
        const value = content.sections[key];
        if (!value && !content.isStreaming) return null;
        return (
          <div key={key} className="stream-section">
            <h4 className="stream-section-label">{t(i18nKeyMap[key])}</h4>
            <div className="stream-section-body">
              {content.source === "vision_capture" &&
                content.aiInferredRanges &&
                key === "original" ? (
                <AiMarkedText
                  text={value ?? ""}
                  ranges={content.aiInferredRanges}
                  aiLabel={t("visionCapture.aiInferred")}
                />
              ) : (
                <p>{value ?? ""}</p>
              )}
              {content.isStreaming && !value && (
                <span className="streaming-cursor" aria-hidden="true" />
              )}
            </div>
          </div>
        );
      })}
      {content.isStreaming && (
        <p className="streaming-indicator" role="status">
          {t("textInsight.analyzing")}
        </p>
      )}
    </div>
  );
}

function AiMarkedText({
  text,
  ranges,
  aiLabel,
}: {
  text: string;
  ranges: Array<{ start: number; end: number }>;
  aiLabel: string;
}) {
  if (!ranges.length) return <p>{text}</p>;

  const parts: Array<{ text: string; isAi: boolean }> = [];
  let cursor = 0;
  const sorted = [...ranges].sort((a, b) => a.start - b.start);

  for (const range of sorted) {
    if (cursor < range.start) {
      parts.push({ text: text.slice(cursor, range.start), isAi: false });
    }
    parts.push({ text: text.slice(range.start, range.end), isAi: true });
    cursor = range.end;
  }
  if (cursor < text.length) {
    parts.push({ text: text.slice(cursor), isAi: false });
  }

  return (
    <p>
      {parts.map((part, i) =>
        part.isAi ? (
          <mark key={i} className="ai-inferred" title={aiLabel}>
            {part.text}
          </mark>
        ) : (
          <span key={i}>{part.text}</span>
        ),
      )}
    </p>
  );
}

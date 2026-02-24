/**
 * Feature: veya-mvp, Property 4: AI 推测内容标记
 *
 * For any recognition result containing AI completion content, the
 * `aiInferredRanges` in the output data structure should accurately mark
 * all AI-inferred content ranges, and marked ranges should not overlap
 * with original OCR content.
 *
 * Validates: Requirements 2.5
 */
import { describe, it, expect } from "vitest";
import * as fc from "fast-check";

/* ------------------------------------------------------------------ */
/*  Types (mirrors design.md StreamContent)                           */
/* ------------------------------------------------------------------ */

interface AiInferredRange {
  start: number;
  end: number;
}

/**
 * Represents a merged recognition result where original OCR segments and
 * AI-inferred segments are concatenated into a single string, with
 * `aiInferredRanges` recording where the AI content lives.
 */
interface RecognitionResult {
  /** The full merged text (OCR + AI segments interleaved). */
  mergedText: string;
  /** Byte-offset ranges within `mergedText` that are AI-inferred. */
  aiInferredRanges: AiInferredRange[];
  /** Byte-offset ranges within `mergedText` that are original OCR. */
  ocrRanges: AiInferredRange[];
}

/* ------------------------------------------------------------------ */
/*  Pure logic under test                                             */
/* ------------------------------------------------------------------ */

/**
 * Merges interleaved OCR and AI segments into a single RecognitionResult.
 * Each segment is tagged with its origin. The function builds the merged
 * string and computes non-overlapping ranges for each origin.
 */
function mergeSegments(
  segments: Array<{ text: string; isAiInferred: boolean }>,
): RecognitionResult {
  let offset = 0;
  const aiInferredRanges: AiInferredRange[] = [];
  const ocrRanges: AiInferredRange[] = [];
  let mergedText = "";

  for (const seg of segments) {
    const start = offset;
    const end = offset + seg.text.length;
    if (seg.text.length > 0) {
      if (seg.isAiInferred) {
        aiInferredRanges.push({ start, end });
      } else {
        ocrRanges.push({ start, end });
      }
    }
    mergedText += seg.text;
    offset = end;
  }

  return { mergedText, aiInferredRanges, ocrRanges };
}

/** Check that two ranges do not overlap. */
function rangesOverlap(a: AiInferredRange, b: AiInferredRange): boolean {
  return a.start < b.end && b.start < a.end;
}

/* ------------------------------------------------------------------ */
/*  Arbitraries                                                       */
/* ------------------------------------------------------------------ */

/** Non-empty string segment (1-50 chars). */
const segmentTextArb = fc
  .string({ minLength: 1, maxLength: 50 })
  .filter((s) => s.length > 0);

/** A single tagged segment. */
const segmentArb = fc.record({
  text: segmentTextArb,
  isAiInferred: fc.boolean(),
});

/**
 * An array of segments that contains at least one AI-inferred segment,
 * ensuring the property is exercised on results with AI content.
 */
const segmentsWithAiArb = fc
  .array(segmentArb, { minLength: 2, maxLength: 10 })
  .filter((segs) => segs.some((s) => s.isAiInferred) && segs.some((s) => !s.isAiInferred));

/* ------------------------------------------------------------------ */
/*  Tests                                                             */
/* ------------------------------------------------------------------ */

describe("Property 4: AI 推测内容标记", () => {
  it("aiInferredRanges cover exactly the AI-inferred portions of the merged text", () => {
    fc.assert(
      fc.property(segmentsWithAiArb, (segments) => {
        const result = mergeSegments(segments);

        // Reconstruct AI text from ranges
        const aiText = result.aiInferredRanges
          .map((r) => result.mergedText.slice(r.start, r.end))
          .join("");

        // Expected AI text from input segments
        const expectedAiText = segments
          .filter((s) => s.isAiInferred)
          .map((s) => s.text)
          .join("");

        expect(aiText).toBe(expectedAiText);
      }),
      { numRuns: 100 },
    );
  });

  it("aiInferredRanges do not overlap with ocrRanges", () => {
    fc.assert(
      fc.property(segmentsWithAiArb, (segments) => {
        const result = mergeSegments(segments);

        for (const aiRange of result.aiInferredRanges) {
          for (const ocrRange of result.ocrRanges) {
            expect(rangesOverlap(aiRange, ocrRange)).toBe(false);
          }
        }
      }),
      { numRuns: 100 },
    );
  });

  it("all ranges have valid bounds within the merged text", () => {
    fc.assert(
      fc.property(segmentsWithAiArb, (segments) => {
        const result = mergeSegments(segments);
        const allRanges = [...result.aiInferredRanges, ...result.ocrRanges];

        for (const range of allRanges) {
          expect(range.start).toBeGreaterThanOrEqual(0);
          expect(range.end).toBeGreaterThan(range.start);
          expect(range.end).toBeLessThanOrEqual(result.mergedText.length);
        }
      }),
      { numRuns: 100 },
    );
  });

  it("aiInferredRanges and ocrRanges together cover the entire merged text without gaps", () => {
    fc.assert(
      fc.property(segmentsWithAiArb, (segments) => {
        const result = mergeSegments(segments);

        // Combine and sort all ranges by start
        const allRanges = [...result.aiInferredRanges, ...result.ocrRanges].sort(
          (a, b) => a.start - b.start,
        );

        // Total covered length should equal merged text length
        const totalCovered = allRanges.reduce((sum, r) => sum + (r.end - r.start), 0);
        expect(totalCovered).toBe(result.mergedText.length);

        // Ranges should be contiguous (no gaps)
        for (let i = 1; i < allRanges.length; i++) {
          expect(allRanges[i].start).toBe(allRanges[i - 1].end);
        }

        // First range starts at 0
        if (allRanges.length > 0) {
          expect(allRanges[0].start).toBe(0);
        }
      }),
      { numRuns: 100 },
    );
  });

  it("no two aiInferredRanges overlap with each other", () => {
    fc.assert(
      fc.property(segmentsWithAiArb, (segments) => {
        const result = mergeSegments(segments);

        for (let i = 0; i < result.aiInferredRanges.length; i++) {
          for (let j = i + 1; j < result.aiInferredRanges.length; j++) {
            expect(
              rangesOverlap(result.aiInferredRanges[i], result.aiInferredRanges[j]),
            ).toBe(false);
          }
        }
      }),
      { numRuns: 100 },
    );
  });
});

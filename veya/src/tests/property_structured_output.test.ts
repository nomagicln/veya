/**
 * Feature: veya-mvp, Property 2: 结构化输出完整性
 *
 * For any text analysis result, the output must contain all six required fields:
 * original, wordByWord, structure, translation, colloquial, simplified,
 * and each field must be a non-empty string.
 *
 * Validates: Requirements 1.3
 */
import { describe, it, expect } from "vitest";
import * as fc from "fast-check";

/** The six required section keys for a structured text insight result. */
const REQUIRED_SECTIONS = [
  "original",
  "wordByWord",
  "structure",
  "translation",
  "colloquial",
  "simplified",
] as const;

type SectionKey = (typeof REQUIRED_SECTIONS)[number];
type StreamSections = Record<SectionKey, string>;

/**
 * Validates that a StreamContent sections object is complete:
 * every required key exists and its value is a non-empty string.
 */
function validateSectionsComplete(sections: StreamSections): boolean {
  return REQUIRED_SECTIONS.every(
    (key) =>
      typeof sections[key] === "string" && sections[key].trim().length > 0,
  );
}

/** Arbitrary that produces a valid non-empty, non-blank string. */
const nonEmptyStringArb = fc
  .string({ minLength: 1, maxLength: 200 })
  .filter((s) => s.trim().length > 0);

/** Arbitrary that produces a complete StreamSections object. */
const streamSectionsArb: fc.Arbitrary<StreamSections> = fc.record({
  original: nonEmptyStringArb,
  wordByWord: nonEmptyStringArb,
  structure: nonEmptyStringArb,
  translation: nonEmptyStringArb,
  colloquial: nonEmptyStringArb,
  simplified: nonEmptyStringArb,
});

describe("Property 2: 结构化输出完整性", () => {
  it("any valid analysis result contains all six required non-empty fields", () => {
    fc.assert(
      fc.property(streamSectionsArb, (sections) => {
        expect(validateSectionsComplete(sections)).toBe(true);

        for (const key of REQUIRED_SECTIONS) {
          expect(typeof sections[key]).toBe("string");
          expect(sections[key].trim().length).toBeGreaterThan(0);
        }
      }),
      { numRuns: 100 },
    );
  });

  it("rejects results with any missing field", () => {
    fc.assert(
      fc.property(
        streamSectionsArb,
        fc.constantFrom(...REQUIRED_SECTIONS),
        (sections, keyToRemove) => {
          const incomplete = { ...sections };
          delete (incomplete as Record<string, unknown>)[keyToRemove];

          const isValid = REQUIRED_SECTIONS.every(
            (key) =>
              typeof (incomplete as Record<string, unknown>)[key] ===
                "string" &&
              ((incomplete as Record<string, unknown>)[key] as string).trim()
                .length > 0,
          );
          expect(isValid).toBe(false);
        },
      ),
      { numRuns: 100 },
    );
  });

  it("rejects results with any empty-string field", () => {
    fc.assert(
      fc.property(
        streamSectionsArb,
        fc.constantFrom(...REQUIRED_SECTIONS),
        (sections, keyToEmpty) => {
          const withEmpty = { ...sections, [keyToEmpty]: "" };

          expect(validateSectionsComplete(withEmpty)).toBe(false);
        },
      ),
      { numRuns: 100 },
    );
  });

  it("rejects results with any whitespace-only field", () => {
    fc.assert(
      fc.property(
        streamSectionsArb,
        fc.constantFrom(...REQUIRED_SECTIONS),
        fc.array(fc.constantFrom(" ", "\t", "\n"), { minLength: 1, maxLength: 10 }).map((chars) => chars.join("")),
        (sections, keyToBlank, whitespace) => {
          const withBlank = { ...sections, [keyToBlank]: whitespace };

          expect(validateSectionsComplete(withBlank)).toBe(false);
        },
      ),
      { numRuns: 100 },
    );
  });
});

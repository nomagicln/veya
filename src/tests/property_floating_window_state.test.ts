/**
 * Feature: veya-mvp, Property 9: 悬浮窗 Pin/隐藏状态机
 *
 * For any floating window state, when the window loses focus:
 * - if pinned is false, the window should auto-hide (visible becomes false)
 * - if pinned is true, the window should remain visible
 * Toggling pin should flip the pinned value.
 *
 * Validates: Requirements 4.2, 4.3
 */
import { describe, it, expect, beforeEach } from "vitest";
import * as fc from "fast-check";
import { useAppStore } from "../store";

/**
 * Pure state machine that mirrors the Zustand store logic for the
 * floating window pin/hide behavior. This lets us test the invariants
 * without needing DOM or Tauri runtime.
 */
interface FloatingWindowVM {
  visible: boolean;
  pinned: boolean;
}

function showWindow(state: FloatingWindowVM): FloatingWindowVM {
  return { ...state, visible: true };
}

function hideWindow(state: FloatingWindowVM): FloatingWindowVM {
  return { ...state, visible: false };
}

function togglePin(state: FloatingWindowVM): FloatingWindowVM {
  return { ...state, pinned: !state.pinned };
}

/** Simulates a blur (focus-lost) event: hides only when not pinned. */
function onBlur(state: FloatingWindowVM): FloatingWindowVM {
  if (!state.pinned) {
    return hideWindow(state);
  }
  return state;
}

type Action = "show" | "blur" | "togglePin";

const actionArb: fc.Arbitrary<Action> = fc.constantFrom("show", "blur", "togglePin");

describe("Property 9: 悬浮窗 Pin/隐藏状态机", () => {
  beforeEach(() => {
    // Reset Zustand store between tests
    useAppStore.setState({
      floatingWindow: {
        visible: false,
        pinned: false,
        position: { x: 0, y: 0 },
        currentContent: null,
        audioState: null,
      },
    });
  });

  it("blur hides the window when not pinned", () => {
    fc.assert(
      fc.property(fc.boolean(), (initialPinned) => {
        const state: FloatingWindowVM = { visible: true, pinned: initialPinned };
        const after = onBlur(state);

        if (initialPinned) {
          expect(after.visible).toBe(true);
        } else {
          expect(after.visible).toBe(false);
        }
      }),
      { numRuns: 100 },
    );
  });

  it("togglePin flips the pinned value", () => {
    fc.assert(
      fc.property(fc.boolean(), (initialPinned) => {
        const state: FloatingWindowVM = { visible: true, pinned: initialPinned };
        const after = togglePin(state);
        expect(after.pinned).toBe(!initialPinned);
      }),
      { numRuns: 100 },
    );
  });

  it("pinned window stays visible through any number of blur events", () => {
    fc.assert(
      fc.property(
        fc.integer({ min: 1, max: 20 }),
        (blurCount) => {
          let state: FloatingWindowVM = { visible: true, pinned: true };
          for (let i = 0; i < blurCount; i++) {
            state = onBlur(state);
          }
          expect(state.visible).toBe(true);
        },
      ),
      { numRuns: 100 },
    );
  });

  it("unpinned window hides on first blur regardless of prior actions", () => {
    fc.assert(
      fc.property(
        fc.array(actionArb, { minLength: 0, maxLength: 15 }),
        (actions) => {
          let state: FloatingWindowVM = { visible: true, pinned: false };

          // Apply random sequence of actions
          for (const action of actions) {
            switch (action) {
              case "show":
                state = showWindow(state);
                break;
              case "blur":
                state = onBlur(state);
                break;
              case "togglePin":
                state = togglePin(state);
                break;
            }
          }

          // Now ensure the window is visible and unpinned, then blur
          state = showWindow(state);
          state = { ...state, pinned: false };
          state = onBlur(state);

          expect(state.visible).toBe(false);
        },
      ),
      { numRuns: 100 },
    );
  });

  it("state machine invariant: after any action sequence, blur hides iff not pinned", () => {
    fc.assert(
      fc.property(
        fc.array(actionArb, { minLength: 1, maxLength: 20 }),
        (actions) => {
          let state: FloatingWindowVM = { visible: true, pinned: false };

          for (const action of actions) {
            switch (action) {
              case "show":
                state = showWindow(state);
                break;
              case "blur":
                state = onBlur(state);
                break;
              case "togglePin":
                state = togglePin(state);
                break;
            }
          }

          // Core invariant: from any reachable state, make visible then blur
          state = showWindow(state);
          const pinnedBefore = state.pinned;
          state = onBlur(state);

          if (pinnedBefore) {
            expect(state.visible).toBe(true);
          } else {
            expect(state.visible).toBe(false);
          }
        },
      ),
      { numRuns: 100 },
    );
  });

  it("Zustand store: togglePin + blur matches pure state machine", () => {
    fc.assert(
      fc.property(
        fc.array(actionArb, { minLength: 1, maxLength: 15 }),
        (actions) => {
          // Reset store
          useAppStore.setState({
            floatingWindow: {
              visible: true,
              pinned: false,
              position: { x: 0, y: 0 },
              currentContent: null,
              audioState: null,
            },
          });

          let pureState: FloatingWindowVM = { visible: true, pinned: false };

          for (const action of actions) {
            switch (action) {
              case "show":
                useAppStore.getState().showWindow();
                pureState = showWindow(pureState);
                break;
              case "blur": {
                const store = useAppStore.getState();
                if (!store.floatingWindow.pinned) {
                  store.hideWindow();
                }
                pureState = onBlur(pureState);
                break;
              }
              case "togglePin":
                useAppStore.getState().togglePin();
                pureState = togglePin(pureState);
                break;
            }
          }

          const storeState = useAppStore.getState().floatingWindow;
          expect(storeState.visible).toBe(pureState.visible);
          expect(storeState.pinned).toBe(pureState.pinned);
        },
      ),
      { numRuns: 100 },
    );
  });
});

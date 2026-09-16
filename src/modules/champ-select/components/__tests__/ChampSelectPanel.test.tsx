// @vitest-environment happy-dom

import { QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { ToastProvider } from "@/components";
import type { ChampSelectView } from "@/lib/tauri";
import { useDialogQueueStore } from "@/stores";
import { mockInvoke } from "@/test/mocks/tauri";
import { createTestQueryClient } from "@/test/utils";

import { useChampSelectStore } from "../../state/champSelect";
import { ChampSelectPanel } from "../ChampSelectPanel";

vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => {}),
}));

function calls(command: string) {
  return mockInvoke.mock.calls.filter(([name]) => name === command);
}

function renderPanel() {
  const queryClient = createTestQueryClient();
  function wrapper({ children }: { children: ReactNode }) {
    return (
      <QueryClientProvider client={queryClient}>
        <ToastProvider>{children}</ToastProvider>
      </QueryClientProvider>
    );
  }
  return render(<ChampSelectPanel />, { wrapper });
}

const KAYN = 141;

function view(overrides: Partial<ChampSelectView> = {}): ChampSelectView {
  return {
    gameId: 1,
    lockedChampionId: null,
    hoveredChampionId: KAYN,
    timerPhase: "BAN_PICK",
    timeLeftMs: 20_000,
    canStillChange: true,
    benchChampionIds: [],
    ...overrides,
  };
}

/** The backend's answers, keyed by command, for a panel that reads four of them. */
function answer(command: string): unknown {
  if (command === "champion_roster") return [{ id: KAYN, name: "Kayn", alias: "Kayn" }];
  if (command === "mods_for_champion") return ["mod-a", "mod-b"];
  if (command === "get_champion_preferences") return {};
  if (command === "get_champion_favorites") return {};
  if (command === "get_champ_select_status") {
    return { wanted: null, applied: null, rebuildMs: 553, tailMs: 3689 };
  }
  if (command === "get_installed_mods") {
    return [
      { id: "mod-a", displayName: "Shadow Kayn", authors: ["Someone"] },
      { id: "mod-b", displayName: "Rhaast Kayn", authors: ["Someone else"] },
    ];
  }
  if (command === "get_mod_thumbnails") return {};
  if (command === "get_patcher_status") return { running: true, phase: "Running", session: null };
  return null;
}

describe("ChampSelectPanel", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockInvoke.mockImplementation((command: string) =>
      Promise.resolve({ ok: true, value: answer(command) }),
    );
    useChampSelectStore.setState({ view: null, lastReport: null, dismissedFor: null });
    useDialogQueueStore.setState({ current: null, claims: [] });
  });

  /// The promise of the feature: a hover is enough, no lock needed.
  it("names the hovered champion and its mods", async () => {
    useChampSelectStore.getState().started(view());
    renderPanel();

    expect(await screen.findByText("Kayn")).toBeVisible();
    expect(await screen.findByText("Shadow Kayn")).toBeVisible();
  });

  /// A select with no champion yet has nothing to offer, and a dialog that
  /// opens on nothing is one the reader closes before it can say anything.
  it("stays down until a champion is in play", async () => {
    useChampSelectStore.getState().started(view({ hoveredChampionId: null }));
    renderPanel();

    /* The claim, not the absence of a title: a dialog whose queries have not
       answered yet also draws no title, so that alone proves nothing. */
    await waitFor(() => expect(useDialogQueueStore.getState().claims).toEqual([]));
    expect(screen.queryByText("Kayn")).toBeNull();
  });

  it("applies a mod for the champion's alias", async () => {
    useChampSelectStore.getState().started(view());
    renderPanel();

    await userEvent.click(await screen.findByText("Shadow Kayn"));

    const call = mockInvoke.mock.calls.find(([name]) => name === "set_champion_preference");
    expect(call?.[1]).toEqual({ champion: "Kayn", modId: "mod-a" });
  });

  it("offers leaving the champion unmodded", async () => {
    useChampSelectStore.getState().started(view());
    renderPanel();

    await userEvent.click(await screen.findByText("No mod"));

    const call = mockInvoke.mock.calls.find(([name]) => name === "set_champion_preference");
    expect(call?.[1]).toEqual({ champion: "Kayn", modId: null });
  });

  /// Closing is about this pick. The next one raises the panel again.
  it("stays closed once dismissed", async () => {
    useChampSelectStore.getState().started(view());
    renderPanel();

    await userEvent.click(await screen.findByText("Close"));

    expect(screen.queryByText("Shadow Kayn")).toBeNull();
  });

  /// The backend raises the window once per champion, so a dismissal that
  /// outlived the champion leaves the window arriving over a panel that will
  /// not open. The two have to agree on what a pick is.
  it("opens again on the champion the reader came back to", async () => {
    useChampSelectStore.getState().started(view());
    renderPanel();

    await userEvent.click(await screen.findByText("Close"));
    useChampSelectStore.getState().changed(view({ hoveredChampionId: 99 }));
    useChampSelectStore.getState().changed(view());

    expect(await screen.findByText("Shadow Kayn")).toBeVisible();
  });

  /// A refusal is the case the panel exists to report honestly.
  it("says why a swap did not happen", async () => {
    useChampSelectStore.getState().started(view());
    useChampSelectStore.getState().reported({
      status: "refused",
      champion: "Kayn",
      modId: "mod-a",
      why: { code: "notEnoughTime", neededMs: 1106, availableMs: 400 },
    });
    renderPanel();

    expect(
      await screen.findByText(/A rebuild needs about 1\.1 s and there were 400 ms left/),
    ).toBeVisible();
  });

  /// The scheduler stays quiet about a refusal a later tick could undo, and an
  /// idle patcher is the one that lasts the whole select. Without this the
  /// panel answers a click with nothing at all.
  it("says the patcher is not running before anything is clicked", async () => {
    mockInvoke.mockImplementation((command: string) => {
      if (command === "get_patcher_status") {
        return Promise.resolve({
          ok: true,
          value: { running: false, phase: "Idle", session: null },
        });
      }
      return Promise.resolve({ ok: true, value: answer(command) });
    });

    useChampSelectStore.getState().started(view());
    renderPanel();

    expect(await screen.findByText(/The patcher isn't running/)).toBeVisible();
  });

  /// The roster arriving late must not read as a champion it cannot name.
  it("waits for the roster before raising anything", async () => {
    let answerRoster = (_: unknown) => {};
    const roster = new Promise((resolve) => {
      answerRoster = resolve;
    });
    mockInvoke.mockImplementation((command: string) => {
      if (command === "champion_roster") return roster;
      return Promise.resolve({ ok: true, value: answer(command) });
    });

    useChampSelectStore.getState().started(view());
    renderPanel();

    await waitFor(() => expect(calls("champion_roster")).toHaveLength(1));
    expect(screen.queryByText("Unrecognised champion")).toBeNull();

    answerRoster({ ok: true, value: answer("champion_roster") });
    expect(await screen.findByText("Kayn")).toBeVisible();
  });

  /// A champion the cached roster cannot name is a roster problem, and saying
  /// so beats drawing an empty list that looks like "no mods for this one".
  it("says when it cannot name the champion", async () => {
    useChampSelectStore.getState().started(view({ hoveredChampionId: 999 }));
    renderPanel();

    expect(await screen.findByText("Unrecognised champion")).toBeVisible();
  });
});

describe("ChampSelectPanel favourites", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockInvoke.mockImplementation((command: string) =>
      Promise.resolve({ ok: true, value: answer(command) }),
    );
    useChampSelectStore.setState({ view: null, lastReport: null, dismissedFor: null });
    useDialogQueueStore.setState({ current: null, claims: [] });
  });

  /// The reader's order, not the library's. A list sorted by anything else
  /// moves the row they were reaching for between one select and the next.
  it("offers favourites first", async () => {
    mockInvoke.mockImplementation((command: string) => {
      if (command === "get_champion_favorites") {
        return Promise.resolve({ ok: true, value: { Kayn: ["mod-b"] } });
      }
      return Promise.resolve({ ok: true, value: answer(command) });
    });

    useChampSelectStore.getState().started(view());
    renderPanel();

    await screen.findByText("Rhaast Kayn");
    const labels = screen
      .getAllByText(/Kayn$/)
      .map((node) => node.textContent)
      .filter((text): text is string => text !== null);
    expect(labels.indexOf("Rhaast Kayn")).toBeLessThan(labels.indexOf("Shadow Kayn"));
  });

  it("marks a mod as a favourite", async () => {
    useChampSelectStore.getState().started(view());
    renderPanel();

    await screen.findByText("Shadow Kayn");
    const stars = await screen.findAllByLabelText("Keep this mod at the top for this champion");
    await userEvent.click(stars[0]!);

    const call = mockInvoke.mock.calls.find(([name]) => name === "set_champion_favorite");
    expect(call?.[1]).toEqual({ champion: "Kayn", modId: "mod-a", favorite: true });
  });

  it("unmarks one that is already a favourite", async () => {
    mockInvoke.mockImplementation((command: string) => {
      if (command === "get_champion_favorites") {
        return Promise.resolve({ ok: true, value: { Kayn: ["mod-a"] } });
      }
      return Promise.resolve({ ok: true, value: answer(command) });
    });

    useChampSelectStore.getState().started(view());
    renderPanel();

    const star = await screen.findByLabelText("Stop keeping this mod at the top");
    await userEvent.click(star);

    const call = mockInvoke.mock.calls.find(([name]) => name === "set_champion_favorite");
    expect(call?.[1]).toEqual({ champion: "Kayn", modId: "mod-a", favorite: false });
  });

  /// Marking a favourite must never write a preference: that is what would
  /// switch off the mods the reader has for the champion.
  it("does not touch what the champion applies", async () => {
    useChampSelectStore.getState().started(view());
    renderPanel();

    await screen.findByText("Shadow Kayn");
    const stars = await screen.findAllByLabelText("Keep this mod at the top for this champion");
    await userEvent.click(stars[0]!);

    expect(calls("set_champion_preference")).toHaveLength(0);
  });

  /// The row that applies no mod is not a mod, so there is nothing to keep.
  it("offers no star on the no-mod row", async () => {
    useChampSelectStore.getState().started(view());
    renderPanel();

    await screen.findByText("No mod");
    expect(
      await screen.findAllByLabelText("Keep this mod at the top for this champion"),
    ).toHaveLength(2);
  });
});

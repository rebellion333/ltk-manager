// @vitest-environment happy-dom

import { QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { ToastProvider } from "@/components";
import { mockInvoke } from "@/test/mocks/tauri";
import { createTestQueryClient } from "@/test/utils";

import { ChampionsBrowser } from "../ChampionsBrowser";

vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => {}),
}));

function renderBrowser() {
  const queryClient = createTestQueryClient();
  function wrapper({ children }: { children: ReactNode }) {
    return (
      <QueryClientProvider client={queryClient}>
        <ToastProvider>{children}</ToastProvider>
      </QueryClientProvider>
    );
  }
  return render(<ChampionsBrowser />, { wrapper });
}

function answer(command: string): unknown {
  if (command === "champions_with_mods") {
    return [
      { champion: { id: 141, name: "Kayn", alias: "Kayn" }, modIds: ["mod-a", "mod-b"] },
      { champion: { id: 17, name: "Teemo", alias: "Teemo" }, modIds: ["mod-a"] },
    ];
  }
  if (command === "mods_for_champion") return ["mod-a", "mod-b"];
  if (command === "get_champion_preferences") return {};
  if (command === "get_champion_favorites") return {};
  if (command === "get_installed_mods") {
    return [
      { id: "mod-a", displayName: "Shadow Kayn", authors: ["Someone"] },
      { id: "mod-b", displayName: "Rhaast Kayn", authors: ["Someone else"] },
    ];
  }
  if (command === "get_mod_thumbnails") return {};
  return null;
}

describe("ChampionsBrowser", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockInvoke.mockImplementation((command: string) =>
      Promise.resolve({ ok: true, value: answer(command) }),
    );
  });

  it("lists the champions the library has mods for", async () => {
    renderBrowser();

    expect(await screen.findByText("Kayn")).toBeVisible();
    expect(screen.getByText("Teemo")).toBeVisible();
  });

  /// The whole point of the page: a preference set outside a champion select.
  it("sets what a champion applies away from a game", async () => {
    renderBrowser();

    await userEvent.click(await screen.findByText("Kayn"));
    await userEvent.click(await screen.findByText("Shadow Kayn"));

    const call = mockInvoke.mock.calls.find(([name]) => name === "set_champion_preference");
    expect(call?.[1]).toEqual({ champion: "Kayn", modId: "mod-a" });
  });

  /// The reason this page exists at all: a favourite could only be unmarked
  /// from the panel, which is gone the moment champion select ends.
  it("unmarks a favourite with no champion select in sight", async () => {
    mockInvoke.mockImplementation((command: string) => {
      if (command === "get_champion_favorites") {
        return Promise.resolve({ ok: true, value: { Kayn: ["mod-a"] } });
      }
      return Promise.resolve({ ok: true, value: answer(command) });
    });

    renderBrowser();
    await userEvent.click(await screen.findByText("Kayn"));

    const star = await screen.findByLabelText("Stop keeping this mod at the top");
    await userEvent.click(star);

    const call = mockInvoke.mock.calls.find(([name]) => name === "set_champion_favorite");
    expect(call?.[1]).toEqual({ champion: "Kayn", modId: "mod-a", favorite: false });
  });

  it("narrows the list as the reader types", async () => {
    renderBrowser();
    await screen.findByText("Kayn");

    await userEvent.type(screen.getByPlaceholderText("Search champions"), "tee");

    expect(screen.queryByText("Kayn")).toBeNull();
    expect(screen.getByText("Teemo")).toBeVisible();
  });
});

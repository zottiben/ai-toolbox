import { act, fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "./api";
import { Update } from "./Update";
import type { UpdateStatus, Updated } from "./types";

vi.mock("./api", () => ({ api: { checkUpdate: vi.fn(), update: vi.fn() } }));

const available: UpdateStatus = {
  current: "0.1.0", latest: "0.2.0", tag: "v0.2.0", available: true,
  release_url: "https://github.com/zottiben/ai-toolbox/releases/tag/v0.2.0",
  installation: {
    binary: "/home/me/.local/bin/ai-toolbox", app: null,
    catalogue: "/home/me/.ai-toolbox/clone", blocked: null, notes: ["Project configuration is left alone."],
  },
  installed: null,
};
const installed: Updated = { version: "0.2.0", restart_required: true, output: "verified and installed" };

beforeEach(() => {
  vi.resetAllMocks();
  vi.mocked(api.checkUpdate).mockResolvedValue(available);
  vi.mocked(api.update).mockResolvedValue(installed);
});

async function check() {
  fireEvent.click(screen.getByRole("button", { name: "Check for updates" }));
  await screen.findByRole("dialog");
}

describe("application updates", () => {
  it("does not contact GitHub or install anything merely by opening the board", () => {
    render(<Update />);
    expect(api.checkUpdate).not.toHaveBeenCalled();
    expect(api.update).not.toHaveBeenCalled();
  });

  it("previews the target and only installs the confirmed release after a second click", async () => {
    render(<Update />);
    await check();
    expect(screen.getByText(available.installation.binary!)).toBeDefined();
    expect(api.update).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Install 0.2.0" }));
    await screen.findByText("Installed ai-toolbox 0.2.0.");
    expect(api.update).toHaveBeenCalledWith("v0.2.0");
    expect(screen.getByText(/Reloading this page alone/)).toBeDefined();
    expect(screen.queryByRole("button", { name: "Install 0.2.0" })).toBeNull();
  });

  it("cancels without a write", async () => {
    render(<Update />);
    await check();
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(api.update).not.toHaveBeenCalled();
  });

  it("does not offer to install when up to date or ahead of the latest release", async () => {
    vi.mocked(api.checkUpdate).mockResolvedValue({ ...available, available: false });
    render(<Update />);
    await check();
    expect(screen.getByText("No newer release available.")).toBeDefined();
    expect(screen.queryByRole("button", { name: /Install/ })).toBeNull();
  });

  it("explains unsupported installation types instead of offering a broken update", async () => {
    vi.mocked(api.checkUpdate).mockResolvedValue({ ...available, installation: { ...available.installation, blocked: "Use your package manager." } });
    render(<Update />);
    await check();
    expect(screen.getByText("Use your package manager.")).toBeDefined();
    expect(screen.queryByRole("button", { name: /Install/ })).toBeNull();
  });

  it("surfaces a failed check and allows retry", async () => {
    vi.mocked(api.checkUpdate).mockRejectedValueOnce(new Error("GitHub is unavailable"));
    render(<Update />);
    fireEvent.click(screen.getByRole("button", { name: "Check for updates" }));
    expect((await screen.findByRole("alert")).textContent).toBe("GitHub is unavailable");
    await check();
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("surfaces installer errors rather than claiming success", async () => {
    vi.mocked(api.update).mockRejectedValueOnce(new Error("checksum mismatch"));
    render(<Update />);
    await check();
    fireEvent.click(screen.getByRole("button", { name: "Install 0.2.0" }));
    expect((await screen.findByRole("alert")).textContent).toBe("checksum mismatch");
    expect(screen.queryByText(/Installed ai-toolbox/)).toBeNull();
    expect((screen.getByRole("button", { name: "Install 0.2.0" }) as HTMLButtonElement).disabled).toBe(false);
  });

  it("disables duplicate clicks and dismissal until installation finishes", async () => {
    let finish!: (result: Updated) => void;
    vi.mocked(api.update).mockReturnValue(new Promise((resolve) => { finish = resolve; }));
    render(<Update />);
    await check();
    fireEvent.click(screen.getByRole("button", { name: "Install 0.2.0" }));
    expect((screen.getByRole("button", { name: "Installing…" }) as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByRole("button", { name: "Close" }) as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "Installing…" }));
    expect(api.update).toHaveBeenCalledTimes(1);
    await act(async () => finish(installed));
  });

  it("remembers a completed update reported by the still-running server", async () => {
    vi.mocked(api.checkUpdate).mockResolvedValue({ ...available, installed });
    render(<Update />);
    await check();
    expect(screen.getByText("Installed ai-toolbox 0.2.0.")).toBeDefined();
    expect(screen.queryByRole("button", { name: /Install 0/ })).toBeNull();
  });
});

import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { api } from "./api";
import { McpPanel } from "./McpPanel";
import type { McpPreview, McpReport } from "./types";
vi.mock("./api", () => ({ api: { mcpReport: vi.fn(), mcpPreview: vi.fn(), mcpApply: vi.fn(), mcpRollback: vi.fn() } }));
const report: McpReport = {
  complete: true, repo: "/fixture", backend: "adapter", known_direct_tools: 0, unknown_catalogues: 1, warnings: [],
  servers: [{ name: "browser", enabled: true, exposure: "direct", source: "/fixture/.mcp.json", exposure_source: "/fixture/.mcp.json", sources: ["/fixture/.mcp.json"], tools: null, direct_tools: null, tool_names: [], cache_note: "Cache identity mismatch", recommend_search: true }],
};
const preview: McpPreview = { id: "retained-id", scope: "project", reports: [report], changes: [{ path: "/fixture/.pi/mcp-adapter.json", server: "browser", before: true, after: "search" }], warnings: ["Reload Pi afterwards."] };
beforeEach(() => {
  vi.resetAllMocks();
  vi.mocked(api.mcpReport).mockResolvedValue(report);
  vi.mocked(api.mcpPreview).mockResolvedValue(preview);
  vi.mocked(api.mcpApply).mockResolvedValue({ id: preview.id, files: 1, backup: "/private/backup.json" });
  vi.mocked(api.mcpRollback).mockResolvedValue({ id: preview.id, files: 1, backup: "/private/backup.json" });
});
async function prepare() {
  render(<McpPanel id={7} onChanged={vi.fn()} />);
  await screen.findByText(/browser — direct, unknown eager tools/);
  fireEvent.click(screen.getByRole("button", { name: "Preview optimisation" }));
  await screen.findByRole("region", { name: "Exact MCP preview" });
}
it("labels unknown counts and sources without performing writes on load", async () => {
  render(<McpPanel id={7} onChanged={vi.fn()} />);
  await screen.findByText(/unknown eager tools/);
  expect(screen.getByText(/Exposure from/).textContent).toContain("/fixture/.mcp.json");
  expect(api.mcpPreview).not.toHaveBeenCalled();expect(api.mcpApply).not.toHaveBeenCalled();
});
it("applies the retained id only after confirmation and offers guarded rollback", async () => {
  await prepare();expect(api.mcpApply).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "Apply this exact preview" }));
  await screen.findByRole("region", { name: "MCP backup" });
  expect(api.mcpApply).toHaveBeenCalledExactlyOnceWith("retained-id");
  expect(screen.queryByRole("region", { name: "Exact MCP preview" })).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Roll back this optimisation" }));
  await vi.waitFor(() => expect(api.mcpRollback).toHaveBeenCalledExactlyOnceWith("retained-id"));
});
it("invalidates the preview whenever scope or retained selections change", async () => {
  await prepare();
  fireEvent.change(screen.getByRole("combobox"), { target: { value: "all" } });
  expect(screen.queryByRole("button", { name: "Apply this exact preview" })).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Preview optimisation" }));
  await screen.findByRole("region", { name: "Exact MCP preview" });
  expect(api.mcpPreview).toHaveBeenLastCalledWith(7, { scope: "all", keep_direct: [], keep_tools: {} });
  fireEvent.click(screen.getByRole("checkbox", { name: "Keep browser unchanged" }));
  expect(screen.queryByRole("button", { name: "Apply this exact preview" })).toBeNull();
  expect(api.mcpApply).not.toHaveBeenCalled();
});
it("cancels a preview without applying it", async () => {
  await prepare();fireEvent.click(screen.getByRole("button", { name: "Cancel preview" }));
  expect(screen.queryByRole("region", { name: "Exact MCP preview" })).toBeNull();expect(api.mcpApply).not.toHaveBeenCalled();
});
it("shows stale-input errors without claiming success", async () => {
  vi.mocked(api.mcpApply).mockRejectedValue(new Error("changed since preview; nothing applied"));
  await prepare();fireEvent.click(screen.getByRole("button", { name: "Apply this exact preview" }));
  expect((await screen.findByRole("alert")).textContent).toContain("changed since preview");
  expect(screen.queryByRole("region", { name: "MCP backup" })).toBeNull();
});

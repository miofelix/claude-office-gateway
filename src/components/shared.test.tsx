// @vitest-environment jsdom
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { CopyRow } from "./shared";
const copy = vi.hoisted(() => {
  Object.defineProperty(window, "localStorage", {
    configurable: true,
    value: { getItem: () => null, setItem: () => undefined },
  });
  return vi.fn();
});
vi.mock("../lib/tauri", () => ({ copyConnectionDetail: copy }));
afterEach(() => {
  cleanup();
  copy.mockReset();
});
it("copies the real native token rather than the hidden presentation label", async () => {
  copy.mockResolvedValue(undefined);
  render(
    <CopyRow
      label="Gateway token"
      value="Stored securely"
      icon={null}
      field="token"
      disabled={false}
      onError={vi.fn()}
    />,
  );
  fireEvent.click(screen.getByRole("button", { name: "Copy Gateway token" }));
  await waitFor(() => expect(copy).toHaveBeenCalledWith("token"));
});
it("reports clipboard failure without claiming success", async () => {
  copy.mockRejectedValue("Clipboard unavailable");
  const error = vi.fn();
  render(
    <CopyRow
      label="Gateway token"
      value="Stored securely"
      icon={null}
      field="token"
      disabled={false}
      onError={error}
    />,
  );
  fireEvent.click(screen.getByRole("button", { name: "Copy Gateway token" }));
  await waitFor(() =>
    expect(error).toHaveBeenCalledWith("Clipboard unavailable"),
  );
});

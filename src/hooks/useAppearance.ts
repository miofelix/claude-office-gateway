import { useEffect, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

export type Appearance = "light" | "dark" | "system";
const storageKey = "office-gateway-appearance";
const query = "(prefers-color-scheme: dark)";
function readAppearance(): Appearance {
  try {
    const saved = window.localStorage?.getItem(storageKey);
    if (saved === "light" || saved === "dark" || saved === "system")
      return saved;
  } catch {}
  return "system";
}
function systemIsDark() {
  return (
    typeof window.matchMedia === "function" && window.matchMedia(query).matches
  );
}
function applyDocumentTheme(theme: "light" | "dark") {
  document.documentElement.dataset.theme = theme;
  document.documentElement.style.colorScheme = theme;
}
export function initializeAppearance() {
  const preference = readAppearance();
  applyDocumentTheme(
    preference === "system" ? (systemIsDark() ? "dark" : "light") : preference,
  );
}
export function useAppearance() {
  const [appearance, setAppearance] = useState<Appearance>(readAppearance);
  const [darkSystem, setDarkSystem] = useState(systemIsDark);
  const theme =
    appearance === "system" ? (darkSystem ? "dark" : "light") : appearance;
  useEffect(() => {
    if (typeof window.matchMedia !== "function") return;
    const media = window.matchMedia(query);
    const update = (event: MediaQueryListEvent) => setDarkSystem(event.matches);
    setDarkSystem(media.matches);
    media.addEventListener("change", update);
    return () => media.removeEventListener("change", update);
  }, []);
  useEffect(() => {
    applyDocumentTheme(theme);
  }, [theme]);
  useEffect(() => {
    try {
      window.localStorage?.setItem(storageKey, appearance);
    } catch {}
    if (isTauri())
      void getCurrentWindow()
        .setTheme(appearance === "system" ? null : appearance)
        .catch((error) =>
          console.warn("Could not apply native window appearance", error),
        );
  }, [appearance]);
  return { appearance, setAppearance, theme };
}

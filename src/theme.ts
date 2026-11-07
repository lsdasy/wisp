import { useEffect, useState } from "react";
import type { ThemeSetting } from "./api";

const media = () => window.matchMedia("(prefers-color-scheme: light)");

/** Итоговая тема с учётом «Системной». Ставит `data-theme` на <html>. */
export function useEffectiveTheme(setting: ThemeSetting | undefined): "dark" | "light" {
  const [systemLight, setSystemLight] = useState(() => media().matches);

  useEffect(() => {
    const m = media();
    const onChange = () => setSystemLight(m.matches);
    m.addEventListener("change", onChange);
    return () => m.removeEventListener("change", onChange);
  }, []);

  const effective = setting === "dark" ? "dark" : setting === "light" ? "light" : systemLight ? "light" : "dark";

  useEffect(() => {
    document.documentElement.dataset.theme = effective;
  }, [effective]);

  return effective;
}

import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { Launcher } from "./launcher/Launcher";
import { SettingsApp } from "./settings/SettingsApp";
import "./styles.css";

// Одна сборка на оба окна: окно настроек открывается с адресом index.html#settings.
const isSettings = window.location.hash === "#settings";
document.documentElement.dataset.view = isSettings ? "settings" : "launcher";

createRoot(document.getElementById("root")!).render(
  <StrictMode>{isSettings ? <SettingsApp /> : <Launcher />}</StrictMode>,
);

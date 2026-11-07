import { useState } from "react";

const MODIFIER_KEYS = new Set(["Control", "Alt", "Shift", "Meta", "OS", "AltGraph"]);

/** Переводит нажатие в формат tauri-plugin-global-shortcut: «Ctrl+Shift+K», «Alt+Space». */
function comboFromEvent(e: React.KeyboardEvent): string | null {
  if (MODIFIER_KEYS.has(e.key)) return null;
  const parts: string[] = [];
  if (e.ctrlKey) parts.push("Ctrl");
  if (e.altKey) parts.push("Alt");
  if (e.shiftKey) parts.push("Shift");
  if (e.metaKey) parts.push("Super");
  if (parts.length === 0) return null; // без модификатора глобальная клавиша сломает обычный ввод
  const code = e.code.replace(/^Key/, "").replace(/^Digit/, "");
  parts.push(code);
  return parts.join("+");
}

export function HotkeyInput({ value, onChange, platform }: { value: string; onChange: (v: string) => void; platform: string }) {
  const [recording, setRecording] = useState(false);
  const display = value
    .split("+")
    .map((p) => (platform === "macos" ? ({ Super: "⌘", Alt: "⌥", Ctrl: "⌃", Shift: "⇧" } as Record<string, string>)[p] ?? p : p === "Super" ? "Win" : p));

  return (
    <button
      type="button"
      className={`hotkey ${recording ? "recording" : ""}`}
      onClick={() => setRecording(true)}
      onBlur={() => setRecording(false)}
      onKeyDown={(e) => {
        if (!recording) return;
        e.preventDefault();
        if (e.key === "Escape") return setRecording(false);
        const combo = comboFromEvent(e);
        if (combo) {
          setRecording(false);
          onChange(combo);
        }
      }}
      aria-label="Горячая клавиша — нажмите, затем введите сочетание"
    >
      {recording ? (
        "Нажмите сочетание… (Esc — отмена)"
      ) : (
        display.map((k, i) => (
          <span key={i} className="hotkey-part">
            {i > 0 && <span className="plus">+</span>}
            <kbd className="kbd">{k}</kbd>
          </span>
        ))
      )}
    </button>
  );
}

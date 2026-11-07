import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api, errorText, EVENTS, type AppInfo, type ExecOutcome, type ItemView, type SearchResponse, type Settings } from "../api";
import { Icon, Kbd } from "../icons";
import { useEffectiveTheme } from "../theme";
import { ActionsMenu, type MenuAction } from "./ActionsMenu";
import { OutputView } from "./OutputView";
import { flatten, ResultList, rowId, type Row } from "./ResultList";

type Output = Extract<ExecOutcome, { type: "output" }>;
type Mode = { kind: "results" } | { kind: "output"; output: Output } | { kind: "plugin"; title: string; items: ItemView[] };

const EMPTY: SearchResponse = { requestId: 0, cancelled: false, calc: null, sections: [] };

export function Launcher() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [query, setQuery] = useState("");
  const [resp, setResp] = useState<SearchResponse>(EMPTY);
  const [selected, setSelected] = useState(0);
  const [mode, setMode] = useState<Mode>({ kind: "results" });
  const [confirm, setConfirm] = useState<{ id: string; title: string } | null>(null);
  const [menu, setMenu] = useState<{ open: boolean; selected: number }>({ open: false, selected: 0 });
  const [notice, setNotice] = useState<{ text: string; error: boolean } | null>(null);
  const [shown, setShown] = useState(false);
  const [refresh, setRefresh] = useState(0);

  const inputRef = useRef<HTMLInputElement>(null);
  const requestRef = useRef(0);
  const noticeTimer = useRef<number | undefined>(undefined);

  const theme = useEffectiveTheme(settings?.theme);
  const mod = info?.platform === "macos" ? "⌘" : "Ctrl";

  // ---------- загрузка, события от Rust ----------
  useEffect(() => {
    api.getSettings().then(setSettings).catch(() => {});
    api.appInfo().then(setInfo).catch(() => {});
  }, []);

  useEffect(() => {
    api.setEffectiveTheme(theme === "dark")
      .then((vibrancy) => setInfo((i) => (i ? { ...i, vibrancy } : i)))
      .catch(() => {});
  }, [theme]);

  useEffect(() => {
    document.documentElement.dataset.platform = info?.platform ?? "";
    document.documentElement.dataset.vibrancy = String(info?.vibrancy ?? false);
  }, [info]);

  const hide = useCallback(() => {
    setShown(false);
    setMenu({ open: false, selected: 0 });
    window.setTimeout(() => api.hideWindow(), settings?.animations === false ? 0 : 140);
  }, [settings?.animations]);

  useEffect(() => {
    const un = [
      listen(EVENTS.shown, () => {
        setQuery("");
        setSelected(0);
        setMode({ kind: "results" });
        setConfirm(null);
        setMenu({ open: false, selected: 0 });
        setRefresh((n) => n + 1);
        setShown(false);
        requestAnimationFrame(() => requestAnimationFrame(() => setShown(true)));
        inputRef.current?.focus();
      }),
      listen(EVENTS.hideRequested, () => hide()),
      listen<Settings>(EVENTS.settingsChanged, (e) => setSettings(e.payload)),
    ];
    // Окно могло быть показано до того, как React подписался на событие.
    requestAnimationFrame(() => setShown(true));
    return () => un.forEach((p) => p.then((f) => f()));
  }, [hide]);

  // ---------- поиск ----------
  useEffect(() => {
    if (mode.kind !== "results") return;
    const id = ++requestRef.current;
    // Небольшая пауза при быстром наборе; старые запросы Rust всё равно отменит сам.
    const t = window.setTimeout(() => {
      api.search(query, id)
        .then((r) => {
          if (r.cancelled || r.requestId !== requestRef.current) return;
          setResp(r);
        })
        .catch((e) => flash(errorText(e), true));
    }, query ? 25 : 0);
    return () => window.clearTimeout(t);
  }, [query, refresh, mode.kind]);

  const rows: Row[] = useMemo(() => {
    if (mode.kind === "plugin") return mode.items.map((item) => ({ type: "item" as const, item }));
    return flatten(resp.calc, resp.sections);
  }, [resp, mode]);

  const sel = rows.length ? Math.min(selected, rows.length - 1) : 0;
  const current = rows[sel];
  const currentItem = current?.type === "item" ? current.item : null;

  function flash(text: string, error = false) {
    setNotice({ text, error });
    window.clearTimeout(noticeTimer.current);
    noticeTimer.current = window.setTimeout(() => setNotice(null), 3500);
  }

  // ---------- действия ----------
  async function run(id: string, confirmed = false) {
    try {
      const o = await api.execute(id, confirmed, query);
      switch (o.type) {
        case "done":
          setConfirm(null);
          if (o.hide) hide();
          else flash(o.message);
          break;
        case "needsConfirmation":
          setConfirm({ id: o.id, title: o.title });
          break;
        case "output":
          setConfirm(null);
          setMode({ kind: "output", output: o });
          break;
        case "items":
          setSelected(0);
          setMode({ kind: "plugin", title: o.title, items: o.items });
          break;
      }
    } catch (e) {
      setConfirm(null);
      flash(errorText(e), true);
    }
  }

  function activate(index: number) {
    const row = rows[index];
    if (!row) return;
    const id = rowId(row);
    run(id, confirm?.id === id);
  }

  async function reveal() {
    if (!currentItem) return;
    try {
      await api.reveal(currentItem.id);
      hide();
    } catch (e) {
      flash(errorText(e), true);
    }
  }

  async function toggleFavorite() {
    if (!currentItem) return;
    try {
      const now = await api.toggleFavorite(currentItem.id);
      flash(now ? "Добавлено в избранное" : "Убрано из избранного");
      setRefresh((n) => n + 1);
    } catch (e) {
      flash(errorText(e), true);
    }
  }

  const canReveal = !!currentItem && ["file", "folder", "app"].includes(currentItem.kind);
  const canFavorite = !!currentItem && mode.kind === "results" && !["shell", "calc"].includes(currentItem.kind);
  const primaryLabel = current?.type === "calc" ? "Скопировать" : currentItem?.actionLabel ?? "Открыть";

  const menuActions: MenuAction[] = [
    { id: "primary", label: primaryLabel, keys: ["↵"] },
    ...(canReveal ? [{ id: "reveal" as const, label: "Показать в папке", keys: [mod, "↵"] }] : []),
    ...(canFavorite
      ? [{ id: "favorite" as const, label: currentItem?.favorite ? "Убрать из избранного" : "В избранное", keys: [mod, "D"] }]
      : []),
  ];

  function pickAction(a: MenuAction) {
    setMenu({ open: false, selected: 0 });
    if (a.id === "primary") activate(sel);
    else if (a.id === "reveal") reveal();
    else toggleFavorite();
  }

  function back() {
    setMode({ kind: "results" });
    setSelected(0);
    inputRef.current?.focus();
  }

  function onKeyDown(e: React.KeyboardEvent<HTMLInputElement>) {
    const modKey = e.ctrlKey || e.metaKey;
    const n = menu.open ? menuActions.length : rows.length;
    const move = (delta: number) => {
      e.preventDefault();
      if (!n) return;
      if (menu.open) setMenu((m) => ({ ...m, selected: (m.selected + delta + n) % n }));
      else {
        setSelected((sel + delta + n) % n);
        setConfirm(null);
      }
    };

    if (e.key === "ArrowDown") return move(1);
    if (e.key === "ArrowUp") return move(-1);

    if (e.key === "Enter") {
      e.preventDefault();
      if (menu.open) return pickAction(menuActions[menu.selected]);
      if (mode.kind === "output") {
        navigator.clipboard?.writeText(mode.output.stdout).then(() => flash("Вывод скопирован"), () => {});
        return;
      }
      if (modKey) return void reveal();
      return activate(sel);
    }

    if (e.key === "Escape") {
      e.preventDefault();
      if (menu.open) return setMenu({ open: false, selected: 0 });
      if (confirm) return setConfirm(null);
      if (mode.kind !== "results") return back();
      if (query) {
        setQuery("");
        setSelected(0);
        return;
      }
      return hide();
    }

    if (modKey && e.key.toLowerCase() === "k") {
      e.preventDefault();
      if (current) setMenu((m) => ({ open: !m.open, selected: 0 }));
      return;
    }
    if (modKey && (e.key.toLowerCase() === "d" || e.code === "KeyD")) {
      e.preventDefault();
      if (canFavorite) toggleFavorite();
      return;
    }
    if (modKey && e.key === ",") {
      e.preventDefault();
      api.openSettings();
    }
  }

  const emptyText =
    query.trim().length === 0 ? (
      <span>Начните вводить: приложение, файл, команду или выражение</span>
    ) : (
      <>
        <span>Ничего не найдено</span>
        <span className="hint">«&gt; команда» — {info?.platform === "windows" ? "PowerShell" : "shell"}, «.pdf» — файлы по расширению, «app -- аргументы» — запуск с аргументами</span>
      </>
    );

  return (
    <div className={`win ${shown ? "shown" : ""} ${settings?.animations === false ? "no-anim" : ""}`} role="dialog" aria-label="Lumen">
      <div className="search">
        {mode.kind === "results" ? (
          <span className="search-icon">{Icon.search()}</span>
        ) : (
          <button type="button" className="icon-btn" onClick={back} aria-label="Назад">
            {Icon.back()}
          </button>
        )}
        <label htmlFor="q" className="sr-only">
          Поиск
        </label>
        <input
          id="q"
          ref={inputRef}
          autoFocus
          autoComplete="off"
          spellCheck={false}
          value={mode.kind === "results" ? query : mode.kind === "plugin" ? mode.title : mode.output.title}
          readOnly={mode.kind !== "results"}
          placeholder="Приложения, файлы, команды, выражения…"
          onChange={(e) => {
            setQuery(e.target.value);
            setSelected(0);
            setConfirm(null);
            setMenu({ open: false, selected: 0 });
          }}
          onKeyDown={onKeyDown}
        />
      </div>

      <div className="body">
        {mode.kind === "output" ? (
          <OutputView output={mode.output} />
        ) : (
          <ResultList
            calc={mode.kind === "results" ? resp.calc : null}
            sections={mode.kind === "results" ? resp.sections : [{ title: mode.title, items: mode.items }]}
            selected={sel}
            confirmId={confirm?.id ?? null}
            onHover={(i) => {
              if (i !== sel) {
                setSelected(i);
                setConfirm(null);
              }
            }}
            onActivate={(i) => {
              setSelected(i);
              activate(i);
              inputRef.current?.focus();
            }}
            empty={emptyText}
          />
        )}
        {menu.open && <ActionsMenu actions={menuActions} selected={menu.selected} onPick={pickAction} />}
      </div>

      <footer className="foot">
        {confirm ? (
          <>
            <span className="foot-item danger strong">
              {Icon.warning()} {confirm.title} — вы уверены?
            </span>
            <span className="foot-group">
              <span className="foot-item">Подтвердить <Kbd>↵</Kbd></span>
              <span className="foot-item">Отмена <Kbd>Esc</Kbd></span>
            </span>
          </>
        ) : (
          <>
            <span className="foot-group">
              <span className="brand">{Icon.logo()} Lumen</span>
              {notice ? (
                <span className={`notice ${notice.error ? "danger" : ""}`} role="status">
                  {notice.text}
                </span>
              ) : (
                <button type="button" className="link-btn" onClick={() => api.openSettings()}>
                  Настройки
                </button>
              )}
            </span>
            <span className="foot-group">
              {mode.kind === "output" ? (
                <>
                  <span className="foot-item">Скопировать вывод <Kbd>↵</Kbd></span>
                  <span className="bar" />
                  <span className="foot-item">Назад <Kbd>Esc</Kbd></span>
                </>
              ) : (
                <>
                  <span className="foot-item strong">{primaryLabel} <Kbd>↵</Kbd></span>
                  <span className="bar" />
                  <span className="foot-item">Действия <Kbd>{mod}</Kbd><Kbd>K</Kbd></span>
                </>
              )}
            </span>
          </>
        )}
      </footer>
    </div>
  );
}

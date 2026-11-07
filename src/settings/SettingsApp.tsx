import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { useEffect, useRef, useState } from "react";
import { api, errorText, EVENTS, type AppInfo, type PluginInfo, type Settings, type Status, type UserCommand } from "../api";
import { Icon } from "../icons";
import { useEffectiveTheme } from "../theme";
import { Row, Segmented, Switch, ToggleRow } from "./controls";
import { HotkeyInput } from "./HotkeyInput";

type Tab = "general" | "search" | "commands" | "plugins" | "security";

const TABS: [Tab, string][] = [
  ["general", "Общие"],
  ["search", "Поиск и индексация"],
  ["commands", "Команды и алиасы"],
  ["plugins", "Расширения"],
  ["security", "Безопасность"],
];

export function SettingsApp() {
  const [tab, setTab] = useState<Tab>("general");
  const [settings, setSettings] = useState<Settings | null>(null);
  const [status, setStatus] = useState<Status | null>(null);
  const [plugins, setPlugins] = useState<PluginInfo[]>([]);
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [message, setMessage] = useState<{ text: string; error: boolean } | null>(null);
  const saveTimer = useRef<number | undefined>(undefined);

  useEffectiveTheme(settings?.theme);

  useEffect(() => {
    api.getSettings().then(setSettings).catch((e) => setMessage({ text: errorText(e), error: true }));
    api.appInfo().then(setInfo).catch(() => {});
    api.listPlugins().then(setPlugins).catch(() => {});
    const un = listen<Settings>(EVENTS.settingsChanged, (e) => setSettings(e.payload));
    return () => void un.then((f) => f());
  }, []);

  // Пока идёт индексация — обновляем счётчики.
  useEffect(() => {
    let alive = true;
    const tick = () => api.getStatus().then((s) => alive && setStatus(s)).catch(() => {});
    tick();
    const t = window.setInterval(tick, 1500);
    return () => {
      alive = false;
      window.clearInterval(t);
    };
  }, []);

  /** Изменения сохраняются сами через 400 мс после последней правки. */
  function update(patch: Partial<Settings>) {
    if (!settings) return;
    const next = { ...settings, ...patch };
    setSettings(next);
    window.clearTimeout(saveTimer.current);
    saveTimer.current = window.setTimeout(async () => {
      try {
        const saved = await api.saveSettings(next);
        setSettings(saved);
        setMessage({ text: "Сохранено", error: false });
        api.listPlugins().then(setPlugins).catch(() => {});
      } catch (e) {
        setMessage({ text: errorText(e), error: true });
        api.getSettings().then(setSettings).catch(() => {});
      }
      window.setTimeout(() => setMessage(null), 2500);
    }, 400);
  }

  if (!settings) {
    return <div className="settings-loading">{message ? message.text : "Загрузка…"}</div>;
  }

  return (
    <div className="settings">
      <nav className="settings-nav" aria-label="Разделы">
        <div className="settings-head">
          <span className="accent">{Icon.logo(18)}</span> Настройки
        </div>
        {TABS.map(([id, label]) => (
          <button key={id} type="button" className="tab" aria-current={tab === id ? "page" : undefined} onClick={() => setTab(id)}>
            {label}
          </button>
        ))}
        <div className="grow" />
        {message && (
          <div className={`save-state ${message.error ? "danger" : ""}`} role="status">
            {message.text}
          </div>
        )}
        <div className="version">Wisp {info?.version}</div>
      </nav>

      <main className="settings-pane">
        {tab === "general" && <General s={settings} update={update} platform={info?.platform ?? ""} />}
        {tab === "search" && <SearchTab s={settings} update={update} status={status} />}
        {tab === "commands" && <Commands s={settings} update={update} />}
        {tab === "plugins" && <Plugins s={settings} update={update} plugins={plugins} setPlugins={setPlugins} status={status} />}
        {tab === "security" && <Security s={settings} update={update} />}
      </main>
    </div>
  );
}

type TabProps = { s: Settings; update: (p: Partial<Settings>) => void };

function General({ s, update, platform }: TabProps & { platform: string }) {
  return (
    <>
      <h2>Общие</h2>
      <p className="lead">Вызов окна, тема и поведение при запуске.</p>
      <Row title="Горячая клавиша" desc="Открывает Wisp поверх всех окон. Нажмите на поле и введите сочетание.">
        <HotkeyInput value={s.hotkey} onChange={(hotkey) => update({ hotkey })} platform={platform} />
      </Row>
      <Row title="Тема" desc="«Системная» следует настройкам Windows / macOS">
        <Segmented
          label="Тема"
          value={s.theme}
          onChange={(theme) => update({ theme })}
          options={[
            ["system", "Системная"],
            ["dark", "Тёмная"],
            ["light", "Светлая"],
          ]}
        />
      </Row>
      <ToggleRow title="Запускать вместе с системой" desc="Wisp стартует свёрнутым в трей" checked={s.autostart} onChange={(autostart) => update({ autostart })} />
      <ToggleRow title="Прятать окно при потере фокуса" checked={s.hideOnBlur} onChange={(hideOnBlur) => update({ hideOnBlur })} />
      <ToggleRow title="Анимации" desc="Плавное появление и скрытие окна" checked={s.animations} onChange={(animations) => update({ animations })} />
    </>
  );
}

function SearchTab({ s, update, status }: TabProps & { status: Status | null }) {
  const [exclude, setExclude] = useState("");
  const providers: [keyof Settings["providers"], string][] = [
    ["apps", "Приложения"],
    ["files", "Файлы"],
    ["commands", "Команды"],
    ["calculator", "Калькулятор"],
    ["plugins", "Расширения"],
  ];

  async function addFolder() {
    const dir = await open({ directory: true, multiple: false, title: "Папка для индексации" });
    if (typeof dir === "string" && !s.indexRoots.includes(dir)) update({ indexRoots: [...s.indexRoots, dir] });
  }

  function addExclude() {
    const v = exclude.trim();
    if (v && !s.indexExcludes.includes(v)) update({ indexExcludes: [...s.indexExcludes, v] });
    setExclude("");
  }

  return (
    <>
      <h2>Поиск и индексация</h2>
      <p className="lead">
        {status
          ? status.files.indexing
            ? "Идёт индексация…"
            : `Приложений: ${status.apps.toLocaleString("ru-RU")} · файлов в индексе: ${status.files.files.toLocaleString("ru-RU")}${status.files.truncated ? " (достигнут предел — уберите лишние папки)" : ""}`
          : "…"}
      </p>

      <div className="block-title">Источники результатов</div>
      <div className="chips-row">
        {providers.map(([key, label]) => (
          <label key={key} className="check">
            <input type="checkbox" checked={s.providers[key]} onChange={(e) => update({ providers: { ...s.providers, [key]: e.target.checked } })} />
            {label}
          </label>
        ))}
      </div>

      <Row title="Результатов в каждой секции">
        <input
          type="number"
          className="input narrow"
          min={1}
          max={20}
          value={s.maxResultsPerSection}
          onChange={(e) => update({ maxResultsPerSection: Number(e.target.value) || 5 })}
        />
      </Row>

      <div className="block-title">Папки</div>
      {s.indexRoots.map((root) => (
        <div key={root} className="folder">
          <span className="muted">{Icon.folder()}</span>
          <span className="mono grow ellipsis">{root}</span>
          <button type="button" className="icon-btn small" aria-label={`Убрать ${root}`} onClick={() => update({ indexRoots: s.indexRoots.filter((r) => r !== root) })}>
            {Icon.close()}
          </button>
        </div>
      ))}
      <div className="btn-row">
        <button type="button" className="btn" onClick={addFolder}>
          + Добавить папку
        </button>
        <button type="button" className="btn ghost" onClick={() => api.reindex()}>
          Переиндексировать
        </button>
      </div>

      <div className="block-title">Исключения</div>
      <p className="hint">Имя папки (node_modules) или маска (*.tmp). Скрытые папки (.git) пропускаются всегда.</p>
      <div className="chips-row">
        {s.indexExcludes.map((x) => (
          <span key={x} className="chip mono">
            {x}
            <button type="button" aria-label={`Убрать ${x}`} onClick={() => update({ indexExcludes: s.indexExcludes.filter((e) => e !== x) })}>
              {Icon.close()}
            </button>
          </span>
        ))}
      </div>
      <div className="btn-row">
        <label htmlFor="exclude" className="sr-only">
          Новое исключение
        </label>
        <input
          id="exclude"
          className="input"
          placeholder="например, build"
          value={exclude}
          onChange={(e) => setExclude(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && addExclude()}
        />
        <button type="button" className="btn" onClick={addExclude}>
          Добавить
        </button>
      </div>
    </>
  );
}

const EMPTY_ALIAS: UserCommand = { keyword: "", title: "", command: "", shell: true, confirm: false };

function Commands({ s, update }: TabProps) {
  const [draft, setDraft] = useState<UserCommand>(EMPTY_ALIAS);
  const valid = draft.keyword.trim() && !/\s/.test(draft.keyword.trim()) && draft.command.trim();

  function add() {
    if (!valid) return;
    const keyword = draft.keyword.trim().toLowerCase();
    update({ aliases: [...s.aliases.filter((a) => a.keyword !== keyword), { ...draft, keyword, title: draft.title.trim() || keyword }] });
    setDraft(EMPTY_ALIAS);
  }

  return (
    <>
      <h2>Команды и алиасы</h2>
      <p className="lead">
        Ключевое слово + команда. Всё, что введено после ключевого слова, подставляется вместо <code>{"{args}"}</code> или дописывается в конец.
      </p>

      {s.aliases.length > 0 && (
        <table className="table">
          <thead>
            <tr>
              <th>Слово</th>
              <th>Название</th>
              <th>Команда</th>
              <th>Shell</th>
              <th>Подтверждать</th>
              <th aria-label="Удалить" />
            </tr>
          </thead>
          <tbody>
            {s.aliases.map((a) => (
              <tr key={a.keyword}>
                <td className="mono">{a.keyword}</td>
                <td>{a.title}</td>
                <td className="mono ellipsis">{a.command}</td>
                <td>{a.shell ? "да" : "нет"}</td>
                <td>{a.confirm ? "да" : "нет"}</td>
                <td>
                  <button type="button" className="icon-btn small" aria-label={`Удалить ${a.keyword}`} onClick={() => update({ aliases: s.aliases.filter((x) => x.keyword !== a.keyword) })}>
                    {Icon.close()}
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}

      <div className="block-title">Новая команда</div>
      <div className="form-grid">
        <label>
          Ключевое слово
          <input className="input mono" placeholder="gs" value={draft.keyword} onChange={(e) => setDraft({ ...draft, keyword: e.target.value })} />
        </label>
        <label>
          Название
          <input className="input" placeholder="Git status" value={draft.title} onChange={(e) => setDraft({ ...draft, title: e.target.value })} />
        </label>
        <label className="wide">
          Команда, путь или ссылка
          <input className="input mono" placeholder="git -C C:\Projects\wisp status" value={draft.command} onChange={(e) => setDraft({ ...draft, command: e.target.value })} />
        </label>
        <label className="check">
          <input type="checkbox" checked={draft.shell} onChange={(e) => setDraft({ ...draft, shell: e.target.checked })} />
          Выполнять в PowerShell / shell и показывать вывод
        </label>
        <label className="check">
          <input type="checkbox" checked={draft.confirm} onChange={(e) => setDraft({ ...draft, confirm: e.target.checked })} />
          Всегда спрашивать подтверждение
        </label>
      </div>
      <div className="btn-row">
        <button type="button" className="btn" disabled={!valid} onClick={add}>
          Добавить команду
        </button>
      </div>
    </>
  );
}

function Plugins({ s, update, plugins, setPlugins, status }: TabProps & { plugins: PluginInfo[]; setPlugins: (p: PluginInfo[]) => void; status: Status | null }) {
  function grant(id: string, patch: Partial<Settings["plugins"][string]>) {
    const current = s.plugins[id] ?? { enabled: false, allowExec: false, allowOpen: false };
    update({ plugins: { ...s.plugins, [id]: { ...current, ...patch } } });
  }

  return (
    <>
      <h2>Расширения</h2>
      <p className="lead">
        Плагин — папка с <code>plugin.json</code>. Новые плагины выключены, а запуск программ и открытие ссылок разрешаются отдельно.
      </p>
      <div className="btn-row">
        <button type="button" className="btn" onClick={() => api.openPluginsDir()}>
          Открыть папку плагинов
        </button>
        <button type="button" className="btn ghost" onClick={() => api.reloadPlugins().then(setPlugins)}>
          Перечитать
        </button>
      </div>
      {status && <p className="hint mono">{status.pluginsDir}</p>}

      {plugins.length === 0 && <p className="hint">Плагинов пока нет. Пример есть в папке examples/plugins репозитория.</p>}

      {plugins.map((p) => {
        const g = s.plugins[p.id] ?? p.grant;
        return (
          <div key={p.id} className="card">
            <div className="card-head">
              <span className="tile glyph">{Icon.puzzle()}</span>
              <div className="grow">
                <div className="set-title">
                  {p.name} <span className="muted">{p.version}</span>
                </div>
                {p.description && <div className="set-desc">{p.description}</div>}
                {p.error && <div className="set-desc danger">{p.error}</div>}
                {!p.error && (
                  <div className="chips-row tight">
                    {p.commands.map((c) => (
                      <span key={c} className="chip mono">
                        {c}
                      </span>
                    ))}
                    {p.permissions.map((perm) => (
                      <span key={perm} className="chip mono perm">
                        {perm}
                      </span>
                    ))}
                  </div>
                )}
              </div>
              <Switch checked={g.enabled} disabled={!!p.error} label={`Включить ${p.name}`} onChange={(enabled) => grant(p.id, { enabled })} />
            </div>
            {!p.error && g.enabled && (
              <div className="card-perms">
                <label className="check">
                  <input type="checkbox" checked={g.allowExec} disabled={!p.permissions.includes("exec")} onChange={(e) => grant(p.id, { allowExec: e.target.checked })} />
                  Разрешить запускать программы
                </label>
                <label className="check">
                  <input type="checkbox" checked={g.allowOpen} disabled={!p.permissions.includes("open")} onChange={(e) => grant(p.id, { allowOpen: e.target.checked })} />
                  Разрешить открывать ссылки
                </label>
              </div>
            )}
          </div>
        );
      })}
    </>
  );
}

function Security({ s, update }: TabProps) {
  const [cleared, setCleared] = useState(false);
  return (
    <>
      <h2>Безопасность</h2>
      <p className="lead">Что Wisp делает без вашего подтверждения. Пароли и токены Wisp не хранит.</p>
      <ToggleRow
        title="Подтверждать опасные операции"
        desc="Выключение, перезагрузка, очистка корзины, команды вроде rm / Remove-Item — только после второго Enter"
        checked={s.confirmDangerous}
        onChange={(confirmDangerous) => update({ confirmDangerous })}
      />
      <ToggleRow title="Показывать вывод команд" desc="Результат PowerShell / shell появляется в окне" checked={s.showCommandOutput} onChange={(showCommandOutput) => update({ showCommandOutput })} />
      <ToggleRow title="Запоминать историю запусков" desc="Недавние и часто используемые результаты поднимаются выше" checked={s.recordHistory} onChange={(recordHistory) => update({ recordHistory })} />
      <div className="btn-row">
        <button
          type="button"
          className="btn ghost"
          onClick={() => api.clearHistory().then(() => setCleared(true))}
        >
          {cleared ? "История очищена" : "Очистить историю"}
        </button>
      </div>
    </>
  );
}

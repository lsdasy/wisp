import { Kbd } from "../icons";

export interface MenuAction {
  id: "primary" | "reveal" | "favorite";
  label: string;
  keys: string[];
}

export function ActionsMenu({ actions, selected, onPick }: { actions: MenuAction[]; selected: number; onPick: (a: MenuAction) => void }) {
  return (
    <div className="actions" role="menu" aria-label="Действия">
      {actions.map((a, i) => (
        <button
          key={a.id}
          type="button"
          role="menuitem"
          className={`action ${i === selected ? "selected" : ""}`}
          onClick={() => onPick(a)}
        >
          <span>{a.label}</span>
          <span className="keys">
            {a.keys.map((k) => (
              <Kbd key={k}>{k}</Kbd>
            ))}
          </span>
        </button>
      ))}
    </div>
  );
}

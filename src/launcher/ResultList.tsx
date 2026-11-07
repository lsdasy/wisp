import { useEffect, useRef } from "react";
import type { CalcView, ItemView, SectionView } from "../api";
import { Icon } from "../icons";

export type Row = { type: "calc"; calc: CalcView } | { type: "item"; item: ItemView };

export const rowId = (r: Row) => (r.type === "calc" ? r.calc.id : r.item.id);

/** Плоский список для навигации стрелками: калькулятор, затем секции по порядку. */
export function flatten(calc: CalcView | null, sections: SectionView[]): Row[] {
  const rows: Row[] = calc ? [{ type: "calc", calc }] : [];
  for (const s of sections) for (const item of s.items) rows.push({ type: "item", item });
  return rows;
}

function ItemIcon({ item }: { item: ItemView }) {
  if (item.tile) {
    return (
      <span className="tile" style={{ background: item.tile.bg, color: item.tile.fg }} aria-hidden="true">
        {item.tile.text}
      </span>
    );
  }
  const glyph =
    item.kind === "file" ? Icon.file()
    : item.kind === "folder" ? Icon.folder()
    : item.kind === "shell" ? Icon.shell()
    : item.kind === "plugin" ? Icon.puzzle()
    : item.danger || item.subtitle === "Система" ? Icon.power()
    : Icon.command();
  return (
    <span className={`tile glyph ${item.danger ? "danger" : ""}`} aria-hidden="true">
      {glyph}
    </span>
  );
}

interface Props {
  calc: CalcView | null;
  sections: SectionView[];
  selected: number;
  confirmId: string | null;
  onHover: (index: number) => void;
  onActivate: (index: number) => void;
  empty: React.ReactNode;
}

export function ResultList({ calc, sections, selected, confirmId, onHover, onActivate, empty }: Props) {
  const listRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    listRef.current?.querySelector('[data-selected="true"]')?.scrollIntoView({ block: "nearest" });
  }, [selected, sections, calc]);

  let index = calc ? 1 : 0;
  const hasAny = !!calc || sections.some((s) => s.items.length > 0);

  return (
    <div className="list" ref={listRef} role="listbox" aria-label="Результаты">
      {calc && (
        <>
          <div className="section-title">Калькулятор</div>
          <button
            type="button"
            className={`calc ${selected === 0 ? "selected" : ""}`}
            data-selected={selected === 0}
            role="option"
            aria-selected={selected === 0}
            onMouseMove={() => onHover(0)}
            onClick={() => onActivate(0)}
          >
            <span className="calc-side">
              <span className="calc-expr mono">{calc.expression}</span>
              <span className="calc-label">Выражение</span>
            </span>
            <span className="calc-arrow">{Icon.arrow()}</span>
            <span className="calc-side right">
              <span className="calc-result">{calc.display}</span>
              <span className="calc-label">{calc.words}</span>
            </span>
          </button>
        </>
      )}

      {sections.map((section) => (
        <div key={section.title} role="group" aria-label={section.title}>
          <div className="section-title">{section.title}</div>
          {section.items.map((item) => {
            const i = index++;
            const isSel = i === selected;
            const confirming = confirmId === item.id;
            return (
              <button
                key={item.id}
                type="button"
                className={`row ${isSel ? "selected" : ""} ${confirming ? "confirming" : ""}`}
                data-selected={isSel}
                role="option"
                aria-selected={isSel}
                onMouseMove={() => onHover(i)}
                onClick={() => onActivate(i)}
              >
                <ItemIcon item={item} />
                <span className="row-main">
                  <span className="row-title">{item.title}</span>
                  <span className="row-sub">{item.subtitle}</span>
                </span>
                {item.favorite && <span className="row-fav" aria-label="В избранном">{Icon.star(true)}</span>}
                <span className={`row-tag ${item.danger ? "danger" : ""}`}>{item.tag}</span>
              </button>
            );
          })}
        </div>
      ))}

      {!hasAny && <div className="empty">{empty}</div>}
    </div>
  );
}
